// TurboGrab — a multi-connection download manager.
//
// One job: take a URL and put the file on disk as fast as the server allows.
// The transfer itself lives in `download.rs`; the loopback API the browser
// extension hands downloads to lives in `server.rs`. This file owns everything
// they share — the item list, the settings, persistence, the queue, the tray,
// and the commands the UI calls.
//
// Lifecycle of an item:
//   queued -> downloading -> done
//                        \-> paused / interrupted -> downloading (resumes from
//                            the exact byte, per connection)
//                        \-> error
//
// Pause writes nothing away: the `.part` and its `.meta` sidecar stay, so a
// resume continues after a pause, a restart, or a crash.

mod download;
mod server;

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;

use download::{Headers, DEFAULT_CONNECTIONS, MAX_CONNECTIONS, MAX_RETRY};

/// Argument the autostart entry launches us with, so a boot-time start goes
/// straight to the tray instead of throwing a window at a user who is still
/// logging in.
const HIDDEN_FLAG: &str = "--hidden";

// Every field added after the first release carries `#[serde(default)]`: a
// `downloads.json` written by an older build must still load, otherwise the
// user's whole history vanishes on upgrade.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct DownloadInfo {
    pub id: String,
    pub url: String,
    pub out_dir: String,
    pub title: String,
    pub percent: f32,
    pub error_msg: String,
    /// "queued" | "downloading" | "paused" | "interrupted" | "done" | "error"
    pub status: String,

    /// Size of the finished file. 0 when the server never said.
    #[serde(default)]
    pub total_bytes: u64,
    /// Bytes on disk right now. Survives a pause, so a resumed card can show
    /// "412 MB / 1.2 GB" instead of a bare percentage.
    #[serde(default)]
    pub downloaded_bytes: u64,
    /// Parallel connections actually in flight (1 = plain single stream).
    #[serde(default)]
    pub connections: u32,
    /// Server honoured a Range request. False means a pause restarts from zero,
    /// which the user deserves to know *before* pausing.
    #[serde(default)]
    pub resumable: bool,
    /// Raw transfer rate in bytes/s, 0 = idle. Formatted in the UI so the units
    /// follow the language.
    #[serde(default)]
    pub speed_bps: f64,
    /// Seconds remaining, -1 = unknown. Same reason as `speed_bps`.
    #[serde(default = "unknown_eta")]
    pub eta_secs: i64,
    /// Current attempt of `retry_max` while recovering a dropped connection.
    /// 0 = not retrying. Without this a backoff looks like a frozen download.
    #[serde(default)]
    pub retry: u32,
    #[serde(default)]
    pub retry_max: u32,
    /// Completion per bucket, 0-100 — the shape of the transfer across its
    /// parallel connections. Live only: cleared before every write to disk.
    ///
    /// Always serialized, empty included. `skip_serializing_if` was tried here
    /// to save a few bytes and made the field vanish from the payload whenever
    /// it was empty — so the UI read `undefined.length` and the render threw.
    /// The wire shape must not depend on the value.
    #[serde(default)]
    pub segments: Vec<u8>,
    /// When the item entered the list. The list sorts on this, so a card never
    /// jumps position just because its status changed.
    #[serde(default)]
    pub created_at: u64,
    /// The untouched failure text behind `error_msg`. Never rendered; offered
    /// as "copy details" so a bug report can still carry the real diagnostic.
    #[serde(default)]
    pub error_detail: String,
    /// Absolute path of the finished file — what "open" and "delete" act on.
    #[serde(default)]
    pub file_path: String,
    /// Unix seconds. 0 = unknown (never started).
    #[serde(default)]
    pub started_at: u64,
    #[serde(default)]
    pub finished_at: u64,
    /// Request headers to replay — cookies, referer, the browser's own
    /// user-agent. Sent by the extension; empty for a link pasted into the app.
    /// This is what makes a download that needs a session work outside the
    /// browser, and it is why they are persisted: a resume tomorrow needs them
    /// as much as the first attempt did.
    #[serde(default)]
    pub headers: Headers,
    /// "app" | "browser" — where the download came from. Shown on the card, so
    /// an item that appeared on its own is explained rather than mysterious.
    #[serde(default)]
    pub source: String,
}

fn unknown_eta() -> i64 {
    -1
}

impl DownloadInfo {
    /// `Default` would leave `eta_secs` at 0, which reads as "finishing now".
    fn blank() -> Self {
        Self {
            eta_secs: -1,
            retry_max: MAX_RETRY,
            created_at: now_secs(),
            source: "app".into(),
            ..Default::default()
        }
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `n` bytes of OS entropy, hex-encoded. Used for the pairing token, which is
/// the only thing standing between the local API and any program on the box —
/// so it comes from the OS RNG, not from a hashed timestamp.
pub fn random_hex(n: usize) -> String {
    let mut buf = vec![0u8; n];
    if getrandom::fill(&mut buf).is_err() {
        // Documented as unreachable on the platforms we ship; still not a
        // reason to hand out a predictable token.
        return String::new();
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

pub struct Item {
    pub info: DownloadInfo,
    /// The transfer runs in an async task that polls this flag. Setting it true
    /// = pause/cancel request. `None` when nothing is running.
    pub cancel: Option<Arc<AtomicBool>>,
    saved_percent: f32,
}

/// Everything the app remembers between launches that is not a download.
///
/// Lives next to `downloads.json` rather than in the webview's localStorage:
/// the destination folder used to be forgotten on every launch while the
/// language persisted, which is exactly the kind of split the user noticed.
/// Rust also needs `lang` for the tray menu, which no localStorage can reach.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Empty on first run; `load_settings` fills it from the OS Downloads dir.
    pub out_dir: String,
    pub lang: String,  // "fr" | "en"
    pub theme: String, // "system" | "light" | "dark"
    /// One-time hint that closing the window only hides it to the tray.
    pub tray_hint_shown: bool,
    /// Connections per download, 1..=16. 1 disables segmenting entirely, which
    /// is the setting to reach for when a server throttles or bans on parallel
    /// requests.
    pub connections: u32,
    /// How many downloads run at once; the rest wait as "queued". Eight
    /// connections each across ten simultaneous files saturates nothing but the
    /// router's NAT table.
    pub max_active: u32,
    /// Start with the session (XDG autostart / LaunchAgent / Run key).
    pub autostart: bool,
    /// Start to the tray with no window. Independent of `autostart`: a manual
    /// launch honours it too.
    pub start_hidden: bool,
    /// Listen for the browser extension.
    pub server_enabled: bool,
    pub server_port: u16,
    /// Pairing token for the local API. Generated on the first successful
    /// pairing, shown in Settings so it can also be pasted in by hand.
    pub token: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            out_dir: String::new(),
            lang: String::new(),
            theme: String::new(),
            tray_hint_shown: false,
            connections: DEFAULT_CONNECTIONS,
            max_active: 3,
            autostart: false,
            start_hidden: false,
            server_enabled: true,
            server_port: server::DEFAULT_PORT,
            token: String::new(),
        }
    }
}

impl Settings {
    /// Clamp anything a hand-edited settings.json could have put out of range.
    fn sane(&mut self) {
        self.connections = self.connections.clamp(1, MAX_CONNECTIONS);
        self.max_active = self.max_active.clamp(1, 10);
        if self.server_port < 1024 {
            self.server_port = server::DEFAULT_PORT;
        }
        if self.lang.is_empty() {
            self.lang = "fr".into();
        }
        if self.theme.is_empty() {
            self.theme = "system".into();
        }
    }
}

/// How many entries the in-memory journal keeps. Enough to cover a session's
/// worth of failures without ever being a memory concern.
const LOG_CAP: usize = 400;

/// One line of the diagnostic journal.
///
/// Cards show a plain sentence and nothing else; the raw text that produced it
/// lands here, so a failure is still explainable after the fact. In memory
/// only: this is for the running session, and a persisted failure already
/// carries its own `error_detail`.
#[derive(Clone, Serialize)]
pub struct LogEntry {
    ts: u64,
    /// "error" | "warn" | "info"
    level: String,
    /// Which part spoke: "download", "link", "server", "app".
    source: String,
    msg: String,
    /// Download this line belongs to, empty when it is app-wide.
    id: String,
}

#[derive(Default)]
pub struct AppState {
    pub items: Mutex<HashMap<String, Item>>,
    pub settings: Mutex<Settings>,
    log: Mutex<VecDeque<LogEntry>>,
    /// Pairing prompts waiting on the user. The HTTP request that opened one is
    /// parked on the other end of the channel.
    pub pending_pairs: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    /// Stops the running listener. Replaced whenever the port changes.
    server_stop: Mutex<Option<oneshot::Sender<()>>>,
    /// Whether anything has ever paired. Only drives the UI badge.
    pub paired: AtomicBool,
    /// This launch should stay in the tray (autostart, or the user's choice).
    hidden_launch: AtomicBool,
}

pub fn state(app: &AppHandle) -> State<'_, AppState> {
    app.state::<AppState>()
}

pub fn log(app: &AppHandle, level: &str, source: &str, id: &str, msg: impl Into<String>) {
    let entry = LogEntry {
        ts: now_secs(),
        level: level.into(),
        source: source.into(),
        msg: msg.into(),
        id: id.into(),
    };
    let state = app.state::<AppState>();
    let mut log = state.log.lock().unwrap();
    if log.len() >= LOG_CAP {
        log.pop_front();
    }
    log.push_back(entry);
}

#[tauri::command]
fn get_log(state: State<AppState>) -> Vec<LogEntry> {
    state.log.lock().unwrap().iter().cloned().collect()
}

#[tauri::command]
fn clear_log(state: State<AppState>) {
    state.log.lock().unwrap().clear();
}

// ---- persistence ----------------------------------------------------------

/// The app's data directory, created if missing and readable only by its owner.
///
/// Both files in here are secrets: `settings.json` holds the pairing token that
/// gates the local API, `downloads.json` the request headers the extension sent
/// — cookies and Authorization among them. On a shared machine the default
/// 0755/0644 hands every other account both.
fn data_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    Some(dir)
}

/// Write a file no other account can read.
///
/// The mode is set at creation rather than chmod'ed afterwards: a chmod leaves a
/// window, however short, in which the token sits on disk world-readable. An
/// existing file keeps the mode it was first created with, hence the re-assert —
/// which is also what repairs a settings.json written by an earlier build.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        f.write_all(bytes)
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes)
    }
}

fn state_file(app: &AppHandle) -> Option<PathBuf> {
    Some(data_dir(app)?.join("downloads.json"))
}

fn save(app: &AppHandle) {
    let Some(path) = state_file(app) else { return };
    let state = app.state::<AppState>();
    let mut infos: Vec<DownloadInfo> = {
        let mut items = state.items.lock().unwrap();
        for it in items.values_mut() {
            it.saved_percent = it.info.percent;
        }
        items.values().map(|it| it.info.clone()).collect()
    };
    // Live-only: a 48-entry array per item, rewritten several times a second,
    // has no business in the on-disk history.
    for i in &mut infos {
        i.segments.clear();
    }
    if let Ok(json) = serde_json::to_vec_pretty(&infos) {
        // Private: the persisted headers are the browser's own cookies.
        let _ = write_private(&path, &json);
    }
}

fn load(app: &AppHandle) {
    let Some(path) = state_file(app) else { return };
    let Ok(bytes) = std::fs::read(path) else { return };
    let Ok(infos) = serde_json::from_slice::<Vec<DownloadInfo>>(&bytes) else { return };

    let state = app.state::<AppState>();
    let mut items = state.items.lock().unwrap();
    for mut info in infos {
        // Nothing is running yet: a download that was live when the app died is
        // interrupted, and one that was still waiting for a slot never started.
        match info.status.as_str() {
            "downloading" | "queued" => {
                info.status = "interrupted".into();
                info.speed_bps = 0.0;
                info.eta_secs = -1;
                info.retry = 0;
                info.connections = 0;
            }
            _ => {}
        }
        if info.retry_max == 0 {
            info.retry_max = MAX_RETRY;
        }
        if info.created_at == 0 {
            info.created_at = info.started_at;
        }
        if info.source.is_empty() {
            info.source = "app".into();
        }
        items.insert(
            info.id.clone(),
            Item {
                saved_percent: info.percent,
                info,
                cancel: None,
            },
        );
    }
}

fn settings_file(app: &AppHandle) -> Option<PathBuf> {
    Some(data_dir(app)?.join("settings.json"))
}

fn load_settings(app: &AppHandle) {
    let mut s: Settings = settings_file(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if s.out_dir.is_empty() {
        s.out_dir = app
            .path()
            .download_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    s.sane();
    app.state::<AppState>().paired.store(!s.token.is_empty(), Ordering::Relaxed);
    *app.state::<AppState>().settings.lock().unwrap() = s;
}

/// Write settings.json from whatever is currently in memory.
pub fn save_settings(app: &AppHandle) {
    let Some(path) = settings_file(app) else { return };
    let json = {
        let state = app.state::<AppState>();
        let cur = state.settings.lock().unwrap();
        match serde_json::to_vec_pretty(&*cur) {
            Ok(j) => j,
            Err(_) => return,
        }
    };
    // Write-then-rename: a crash mid-write must not leave a truncated file
    // that resets every preference on the next launch.
    // The temp file is created private too — the rename would otherwise carry a
    // world-readable token into place.
    let tmp = path.with_extension("json.tmp");
    if write_private(&tmp, &json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn set_settings(app: AppHandle, state: State<AppState>, settings: Settings) -> Result<(), String> {
    let (lang_changed, server_changed, autostart_changed, max_active_up) = {
        let mut cur = state.settings.lock().unwrap();
        let mut next = settings;
        next.sane();
        // The token is never handed to the frontend to rewrite: pairing owns it.
        next.token = cur.token.clone();
        let flags = (
            cur.lang != next.lang,
            cur.server_enabled != next.server_enabled || cur.server_port != next.server_port,
            cur.autostart != next.autostart,
            next.max_active > cur.max_active,
        );
        *cur = next;
        flags
    };
    save_settings(&app);
    if lang_changed {
        retray(&app)?;
    }
    if server_changed {
        restart_server(&app);
    }
    if autostart_changed {
        apply_autostart(&app);
    }
    if max_active_up {
        pump(&app); // freed slots start waiting downloads right away
    }
    Ok(())
}

// ---- the queue ------------------------------------------------------------

/// Downloads actually moving bytes right now.
fn active_count(items: &HashMap<String, Item>) -> usize {
    items.values().filter(|it| it.info.status == "downloading").count()
}

/// Start a download, or park it as "queued" when every slot is busy.
///
/// Every entry point goes through here — the composer, a resume, the browser
/// extension — so the concurrency limit holds however a download arrives.
fn start_or_queue(app: &AppHandle, id: &str) -> Result<(), String> {
    let queued = {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        let busy = active_count(&items) >= max_active(app) as usize;
        let it = items.get_mut(id).ok_or("unknown download")?;
        if busy {
            it.info.status = "queued".into();
            it.info.error_msg.clear();
            let _ = app.emit("download-update", it.info.clone());
        }
        busy
    };
    if queued {
        save(app);
        return Ok(());
    }
    spawn_download(app, id)
}

fn max_active(app: &AppHandle) -> u32 {
    app.state::<AppState>().settings.lock().unwrap().max_active
}

/// Fill every free slot from the queue, oldest first.
///
/// Called whenever a slot could have opened: a download finished, failed, was
/// paused, was removed, or the limit was raised.
fn pump(app: &AppHandle) {
    loop {
        let next = {
            let state = app.state::<AppState>();
            let items = state.items.lock().unwrap();
            if active_count(&items) >= max_active(app) as usize {
                return;
            }
            let mut queued: Vec<&Item> = items
                .values()
                .filter(|it| it.info.status == "queued")
                .collect();
            queued.sort_by_key(|it| (it.info.created_at, it.info.id.clone()));
            queued.first().map(|it| it.info.id.clone())
        };
        let Some(id) = next else { return };
        if spawn_download(app, &id).is_err() {
            return; // don't spin on an item that refuses to start
        }
    }
}

fn spawn_download(app: &AppHandle, id: &str) -> Result<(), String> {
    let cancel = Arc::new(AtomicBool::new(false));
    let connections = {
        let state = app.state::<AppState>();
        let conns = state.settings.lock().unwrap().connections;
        let mut items = state.items.lock().unwrap();
        let it = items.get_mut(id).ok_or("unknown download")?;
        it.cancel = Some(cancel.clone());
        it.info.status = "downloading".into();
        it.info.error_msg.clear();
        it.info.error_detail.clear();
        let _ = app.emit("download-update", it.info.clone());
        conns
    };
    save(app);

    let app = app.clone();
    let id = id.to_string();
    tauri::async_runtime::spawn(async move {
        match download::run_file_download(&app, &id, connections, cancel).await {
            Ok(true) => finish_file(&app, &id),
            Ok(false) => {} // paused/cancelled — status already set by the command
            Err(e) => fail_file(&app, &id, &e),
        }
        // Whatever the outcome, this slot is free now.
        pump(&app);
    });
    Ok(())
}

// ---- progress -------------------------------------------------------------

/// Mutate one item's info and push it to the UI. Every detail field goes
/// through here, so there is one place that emits.
pub fn patch(app: &AppHandle, id: &str, f: impl FnOnce(&mut DownloadInfo)) {
    let state = app.state::<AppState>();
    let mut items = state.items.lock().unwrap();
    if let Some(it) = items.get_mut(id) {
        f(&mut it.info);
        let _ = app.emit("download-update", it.info.clone());
    }
}

pub fn set_title(app: &AppHandle, id: &str, title: &str) {
    patch(app, id, |i| i.title = title.to_string());
    save(app);
}

pub fn update_file_progress(
    app: &AppHandle,
    id: &str,
    downloaded: u64,
    total: u64,
    speed: f64,
    segments: Vec<u8>,
) {
    let mut should_save = false;
    {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        let Some(it) = items.get_mut(id) else { return };
        if total > 0 {
            it.info.percent = (downloaded as f32 / total as f32) * 100.0;
            it.info.total_bytes = total;
        }
        it.info.downloaded_bytes = downloaded;
        it.info.retry = 0; // bytes are moving again
        it.info.speed_bps = speed.max(0.0);
        it.info.segments = segments;
        let remaining = total.saturating_sub(downloaded) as f64;
        it.info.eta_secs = if speed > 1.0 && total > 0 {
            (remaining / speed) as i64
        } else {
            -1
        };
        if it.info.percent - it.saved_percent >= 2.0 {
            should_save = true;
        }
        let _ = app.emit("download-update", it.info.clone());
    }
    if should_save {
        save(app); // save() resets saved_percent for every item
    }
}

fn finish_file(app: &AppHandle, id: &str) {
    {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        let Some(it) = items.get_mut(id) else { return };
        it.cancel = None;
        it.info.status = "done".into();
        it.info.percent = 100.0;
        it.info.retry = 0;
        it.info.finished_at = now_secs();
        if it.info.total_bytes > 0 {
            it.info.downloaded_bytes = it.info.total_bytes;
        }
        it.info.speed_bps = 0.0;
        it.info.eta_secs = -1;
        it.info.connections = 0;
        it.info.segments.clear();
        let _ = app.emit("download-update", it.info.clone());
    }
    save(app);
}

fn fail_file(app: &AppHandle, id: &str, msg: &str) {
    let mut reported = false;
    {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        let Some(it) = items.get_mut(id) else { return };
        it.cancel = None;
        // A pause races the stream: don't clobber "paused" with "error".
        if it.info.status != "paused" {
            it.info.status = "error".into();
            it.info.error_msg = download::classify(msg, "@download_failed");
            it.info.error_detail =
                if it.info.error_msg == msg { String::new() } else { msg.to_string() };
            it.info.speed_bps = 0.0;
            it.info.eta_secs = -1;
            it.info.connections = 0;
            reported = true;
            let _ = app.emit("download-update", it.info.clone());
        }
    }
    if reported {
        log(app, "error", "download", id, msg);
    }
    save(app);
}

// ---- adding ---------------------------------------------------------------

/// Insert a fresh item and hand it to the queue.
fn add(app: &AppHandle, info: DownloadInfo) -> Result<String, String> {
    let id = info.id.clone();
    {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        // Emit right away so the card exists the moment the user hits Download.
        let _ = app.emit("download-update", info.clone());
        items.insert(
            id.clone(),
            Item { info, cancel: None, saved_percent: 0.0 },
        );
    }
    save(app);
    start_or_queue(app, &id)?;
    Ok(id)
}

/// Add one or more links pasted into the app. Returns the ids created.
///
/// Plural on purpose: a pasted block of links used to need one paste, one
/// click, one card at a time.
#[tauri::command]
fn add_downloads(app: AppHandle, urls: Vec<String>, out_dir: String) -> Result<Vec<String>, String> {
    if out_dir.trim().is_empty() {
        return Err("@no_dir".into());
    }
    let mut ids = Vec::new();
    for url in urls {
        let url = url.trim().to_string();
        if url.is_empty() {
            continue;
        }
        let info = DownloadInfo {
            id: random_hex(8),
            url,
            out_dir: out_dir.clone(),
            status: "queued".into(),
            ..DownloadInfo::blank()
        };
        ids.push(add(&app, info)?);
    }
    if ids.is_empty() {
        return Err("@bad_link".into());
    }
    Ok(ids)
}

/// A download handed over by the browser extension.
///
/// Runs on the HTTP thread, so it never blocks on the UI: the card appears, the
/// queue decides whether it starts now, and the extension gets its id back.
pub fn add_from_browser(
    app: &AppHandle,
    url: String,
    filename: String,
    headers: Headers,
    size: u64,
) -> Result<String, String> {
    let out_dir = {
        let state = app.state::<AppState>();
        let s = state.settings.lock().unwrap();
        s.out_dir.clone()
    };
    if out_dir.is_empty() {
        return Err("@no_dir".into());
    }
    // The browser's suggested name is a filename, not a path: it goes through
    // the same sanitizer as a server-supplied one before it can name a file.
    let title = download::sanitize(&filename);
    let title = if title.is_empty() {
        String::new()
    } else {
        download::unique_name(&PathBuf::from(&out_dir), &title)
    };
    let info = DownloadInfo {
        id: random_hex(8),
        url,
        out_dir,
        title,
        total_bytes: size,
        headers,
        source: "browser".into(),
        status: "queued".into(),
        ..DownloadInfo::blank()
    };
    let id = add(app, info)?;
    // A download that appears from nowhere while the window is hidden is worth
    // a tray notification at most — never a window stealing focus mid-browse.
    log(app, "info", "server", &id, "download received from the browser");
    Ok(id)
}

/// Compact status list for the extension's popup.
#[derive(Serialize)]
pub struct BrowserItem {
    id: String,
    title: String,
    status: String,
    percent: f32,
    speed_bps: f64,
    total_bytes: u64,
    downloaded_bytes: u64,
}

pub fn browser_summary(app: &AppHandle) -> Vec<BrowserItem> {
    let state = app.state::<AppState>();
    let items = state.items.lock().unwrap();
    let mut out: Vec<BrowserItem> = items
        .values()
        .map(|it| BrowserItem {
            id: it.info.id.clone(),
            title: it.info.title.clone(),
            status: it.info.status.clone(),
            percent: it.info.percent,
            speed_bps: it.info.speed_bps,
            total_bytes: it.info.total_bytes,
            downloaded_bytes: it.info.downloaded_bytes,
        })
        .collect();
    out.sort_by(|a, b| b.percent.partial_cmp(&a.percent).unwrap_or(std::cmp::Ordering::Equal));
    out
}

// ---- commands -------------------------------------------------------------

#[tauri::command]
fn pause_download(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    {
        let mut items = state.items.lock().unwrap();
        let it = items.get_mut(&id).ok_or("unknown download")?;
        it.info.status = "paused".into();
        it.info.speed_bps = 0.0;
        it.info.eta_secs = -1;
        it.info.connections = 0;
        if let Some(flag) = &it.cancel {
            flag.store(true, Ordering::Relaxed);
        }
        let _ = app.emit("download-update", it.info.clone());
    }
    save(&app);
    // The paused download's slot belongs to whatever is waiting.
    pump(&app);
    Ok(())
}

#[tauri::command]
fn resume_download(app: AppHandle, id: String) -> Result<(), String> {
    start_or_queue(&app, &id)
}

#[tauri::command]
fn resume_all(app: AppHandle, state: State<AppState>) -> Result<Vec<String>, String> {
    let ids: Vec<String> = {
        let items = state.items.lock().unwrap();
        let mut waiting: Vec<&Item> = items
            .values()
            .filter(|it| it.info.status == "interrupted" || it.info.status == "paused")
            .collect();
        waiting.sort_by_key(|it| (it.info.created_at, it.info.id.clone()));
        waiting.iter().map(|it| it.info.id.clone()).collect()
    };
    for id in &ids {
        let _ = start_or_queue(&app, id);
    }
    Ok(ids)
}

/// Point an existing download at a fresh URL and restart it, keeping the bytes
/// already on disk. This is the answer to an expired signed link: the
/// `.part`/`.meta` are deliberately left alone, and the stored `If-Range`
/// validator decides on the next request whether they can be resumed onto (same
/// file -> continue where it stopped) or must be discarded (different file ->
/// clean restart). Nothing here has to trust that the user pasted the right link.
#[tauri::command]
fn refresh_link(
    app: AppHandle,
    state: State<AppState>,
    id: String,
    url: String,
) -> Result<(), String> {
    let url = url.trim().to_string();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("@bad_link".into());
    }
    {
        let mut items = state.items.lock().unwrap();
        let it = items.get_mut(&id).ok_or("unknown download")?;
        // Swapping the URL under a live task would leave the old workers writing
        // one file's bytes into another file's `.part`. Pause first.
        if it.info.status == "downloading" {
            return Err("@busy".into());
        }
        it.info.url = url;
        it.info.error_msg.clear();
    }
    save(&app);
    start_or_queue(&app, &id)
}

/// Delete the finished file plus any leftover `.part` / `.meta` sidecars.
fn remove_files(info: &DownloadInfo) {
    if info.out_dir.is_empty() || info.title.is_empty() {
        return;
    }
    // `file_path` is authoritative once the transfer named it; `out_dir + title`
    // is the fallback for items saved by older builds.
    let base = if info.file_path.is_empty() {
        PathBuf::from(&info.out_dir).join(&info.title)
    } else {
        PathBuf::from(&info.file_path)
    };
    let _ = std::fs::remove_file(&base);
    let part = PathBuf::from(&info.out_dir).join(format!("{}.part", info.title));
    let _ = std::fs::remove_file(&part);
    let _ = std::fs::remove_file(download::meta_path(&part));
}

#[tauri::command]
fn cancel_download(
    app: AppHandle,
    state: State<AppState>,
    id: String,
    delete_file: bool,
) -> Result<(), String> {
    {
        let mut items = state.items.lock().unwrap();
        if let Some(it) = items.remove(&id) {
            if let Some(flag) = &it.cancel {
                flag.store(true, Ordering::Relaxed);
            }
            if delete_file {
                remove_files(&it.info);
            }
        }
    }
    let _ = app.emit("download-removed", id);
    save(&app);
    pump(&app);
    Ok(())
}

#[tauri::command]
fn clear_finished(
    app: AppHandle,
    state: State<AppState>,
    delete_files: bool,
) -> Result<(), String> {
    {
        let mut items = state.items.lock().unwrap();
        let done: Vec<String> = items
            .values()
            .filter(|it| it.info.status == "done" || it.info.status == "error")
            .map(|it| it.info.id.clone())
            .collect();
        for id in done {
            if let Some(it) = items.remove(&id) {
                if delete_files {
                    remove_files(&it.info);
                }
            }
        }
    }
    save(&app);
    Ok(())
}

/// Absolute path of an item's file, if it is on disk.
///
/// Resolved here rather than in the frontend so the opener only ever sees a
/// path the app itself produced — no wildcard `open-path` scope is handed to
/// the webview.
fn item_path(state: &State<AppState>, id: &str) -> Option<PathBuf> {
    let items = state.items.lock().unwrap();
    let info = &items.get(id)?.info;
    let path = if info.file_path.is_empty() {
        if info.out_dir.is_empty() || info.title.is_empty() {
            return None;
        }
        PathBuf::from(&info.out_dir).join(&info.title)
    } else {
        PathBuf::from(&info.file_path)
    };
    path.is_file().then_some(path)
}

/// Open a finished download in the system's default application.
#[tauri::command]
fn open_file(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    let path = item_path(&state, &id).ok_or("@missing_file")?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Show a finished download selected in the system file manager.
#[tauri::command]
fn reveal_file(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    let path = item_path(&state, &id).ok_or("@missing_file")?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| e.to_string())
}

/// Whether each id still has its file on disk, so the UI can hide "open" on a
/// download the user deleted behind the app's back.
#[tauri::command]
fn files_present(state: State<AppState>, ids: Vec<String>) -> Vec<bool> {
    ids.iter().map(|id| item_path(&state, id).is_some()).collect()
}

#[tauri::command]
fn list_downloads(state: State<AppState>) -> Vec<DownloadInfo> {
    state
        .items
        .lock()
        .unwrap()
        .values()
        .map(|it| it.info.clone())
        .collect()
}

// ---- browser integration --------------------------------------------------

fn restart_server(app: &AppHandle) {
    let (enabled, port) = {
        let state = app.state::<AppState>();
        let s = state.settings.lock().unwrap();
        (s.server_enabled, s.server_port)
    };
    // Dropping the previous sender is what stops the old listener; do that
    // before binding again, or the new one lands on a busy port.
    {
        let state = app.state::<AppState>();
        let mut stop = state.server_stop.lock().unwrap();
        if let Some(tx) = stop.take() {
            let _ = tx.send(());
        }
    }
    if !enabled {
        let _ = app.emit("server-state", serde_json::json!({
            "running": false, "port": port, "error": ""
        }));
        return;
    }
    let tx = server::start(app, port);
    *app.state::<AppState>().server_stop.lock().unwrap() = Some(tx);
}

/// Answer a pairing prompt. `allow` false (or a closed dialog) denies it.
#[tauri::command]
fn pair_respond(app: AppHandle, state: State<AppState>, id: String, allow: bool) {
    let tx = state.pending_pairs.lock().unwrap().remove(&id);
    if let Some(tx) = tx {
        let _ = tx.send(allow);
    }
    if allow {
        // The token is minted by the pairing task; tell the UI once it exists.
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            let token = app.state::<AppState>().settings.lock().unwrap().token.clone();
            let _ = app.emit("paired", token);
        });
    }
}

/// The token, for pasting into an extension that can't pair interactively.
#[tauri::command]
fn get_token(state: State<AppState>) -> String {
    state.settings.lock().unwrap().token.clone()
}

/// Mint a token without a pairing prompt — the manual path, for a user who
/// would rather copy it into the extension than approve a dialog.
#[tauri::command]
fn ensure_token(app: AppHandle, state: State<AppState>) -> String {
    let token = {
        let mut s = state.settings.lock().unwrap();
        if s.token.is_empty() {
            s.token = random_hex(16);
        }
        s.token.clone()
    };
    save_settings(&app);
    state.paired.store(true, Ordering::Relaxed);
    token
}

/// Revoke the token: every paired extension stops being able to add downloads.
#[tauri::command]
fn revoke_token(app: AppHandle, state: State<AppState>) {
    state.settings.lock().unwrap().token.clear();
    state.paired.store(false, Ordering::Relaxed);
    save_settings(&app);
    let _ = app.emit("paired", String::new());
}

/// Whether the listener is up, so Settings can say so plainly.
#[tauri::command]
fn server_state(state: State<AppState>) -> serde_json::Value {
    let s = state.settings.lock().unwrap();
    serde_json::json!({
        "enabled": s.server_enabled,
        "port": s.server_port,
        "paired": !s.token.is_empty(),
    })
}

// ---- startup behaviour ----------------------------------------------------

/// Did this launch ask to stay in the tray? The frontend asks before showing
/// the window, which it otherwise does on its first paint.
#[tauri::command]
fn launched_hidden(state: State<AppState>) -> bool {
    state.hidden_launch.load(Ordering::Relaxed)
}

/// Push the "launch on boot" setting to the OS autostart entry.
fn apply_autostart(app: &AppHandle) {
    let want = app.state::<AppState>().settings.lock().unwrap().autostart;
    let mgr = app.autolaunch();
    let res = if want { mgr.enable() } else { mgr.disable() };
    if let Err(e) = res {
        log(app, "error", "app", "", format!("autostart: {e}"));
    }
}

// ---- system tray ----------------------------------------------------------

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// The tray menu is the only UI outside the webview, so it carries its own
/// two-string dictionary rather than reaching into the frontend's.
fn tray_labels(lang: &str) -> (&'static str, &'static str) {
    match lang {
        "en" => ("Show TurboGrab", "Quit"),
        _ => ("Afficher TurboGrab", "Quitter"),
    }
}

fn tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let lang = app.state::<AppState>().settings.lock().unwrap().lang.clone();
    let (show_label, quit_label) = tray_labels(&lang);
    let show = MenuItem::with_id(app, "show", show_label, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;
    Menu::with_items(app, &[&show, &quit])
}

/// Rebuild the tray menu in place after a language change.
fn retray(app: &AppHandle) -> Result<(), String> {
    let menu = tray_menu(app).map_err(|e| e.to_string())?;
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = tray_menu(app)?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("TurboGrab")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Left click toggles the window (hide if visible, else show).
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    if w.is_visible().unwrap_or(false) {
                        let _ = w.hide();
                    } else {
                        show_main(app);
                    }
                }
            }
        })
        .build(app)?;
    Ok(())
}

/// Work around a WebKitGTK crash that blanks the window.
///
/// On hybrid Intel + NVIDIA machines the DMABUF renderer aborts inside Mesa's
/// GBM teardown — the backtrace runs `exit -> libwebkit2gtk -> libgbm ->
/// dri_gbm -> libgallium -> free()` and glibc kills the process with
/// "corrupted double-linked list". The web process dies, the UI process keeps
/// running, and the user is left staring at a blank white webview.
///
/// Disabling the DMABUF path costs a little compositing efficiency, which a
/// downloader's UI will never notice. Only set when the user hasn't already
/// chosen a value, so `WEBKIT_DISABLE_DMABUF_RENDERER=0` still wins.
#[cfg(target_os = "linux")]
fn avoid_dmabuf_crash() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

/// Links passed on the command line: `turbograb https://…` queues them.
/// Also how a second instance's arguments reach the running one.
fn queue_cli_urls(app: &AppHandle, args: &[String]) {
    let urls: Vec<String> = args
        .iter()
        .filter(|a| a.starts_with("http://") || a.starts_with("https://"))
        .cloned()
        .collect();
    if urls.is_empty() {
        return;
    }
    let out_dir = app.state::<AppState>().settings.lock().unwrap().out_dir.clone();
    let _ = add_downloads(app.clone(), urls, out_dir);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Must happen before the webview is created, so before the Builder runs.
    #[cfg(target_os = "linux")]
    avoid_dmabuf_crash();

    tauri::Builder::default()
        // First, so a second launch hands its arguments over and exits rather
        // than fighting the running instance for the API port.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            show_main(app);
            queue_cli_urls(app, &argv);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            // The boot-time launch goes to the tray. Without this, logging in
            // would throw a download manager's window in your face.
            Some(vec![HIDDEN_FLAG]),
        ))
        .manage(AppState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            load_settings(&handle);
            load(&handle);
            build_tray(&handle)?;

            let args: Vec<String> = std::env::args().skip(1).collect();
            let hidden = args.iter().any(|a| a == HIDDEN_FLAG)
                || handle.state::<AppState>().settings.lock().unwrap().start_hidden;
            handle.state::<AppState>().hidden_launch.store(hidden, Ordering::Relaxed);

            apply_autostart(&handle);
            restart_server(&handle);
            queue_cli_urls(&handle, &args);

            // The window ships hidden and the frontend reveals it after its
            // first paint, so a borderless window never flashes the wrong
            // colour. If the frontend fails to boot, that would leave the app
            // running with nothing on screen — so show it anyway after a beat,
            // unless this launch was meant to stay in the tray.
            if !hidden {
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    if let Some(w) = handle.get_webview_window("main") {
                        if !w.is_visible().unwrap_or(true) {
                            let _ = w.show();
                        }
                    }
                });
            }
            Ok(())
        })
        // Closing the window hides it to the tray instead of quitting; use the
        // tray's "Quit" to exit for real.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            add_downloads,
            pause_download,
            refresh_link,
            resume_download,
            resume_all,
            cancel_download,
            clear_finished,
            list_downloads,
            open_file,
            reveal_file,
            files_present,
            get_settings,
            set_settings,
            get_log,
            clear_log,
            pair_respond,
            get_token,
            ensure_token,
            revoke_token,
            server_state,
            launched_hidden,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- the wire contract ----------------------------------------------

    #[test]
    fn every_download_info_field_is_always_serialized() {
        // The UI types this payload as fully populated. A field that vanishes
        // when empty turns into `undefined` in the webview, and the render
        // throws on the first `.length` or `.toFixed` it reaches.
        let v = serde_json::to_value(DownloadInfo::blank()).unwrap();
        let obj = v.as_object().unwrap();
        for field in [
            "id", "url", "out_dir", "title", "percent", "error_msg", "status",
            "total_bytes", "downloaded_bytes", "connections", "resumable",
            "speed_bps", "eta_secs", "retry", "retry_max", "segments",
            "created_at", "error_detail", "file_path", "started_at",
            "finished_at", "headers", "source",
        ] {
            assert!(obj.contains_key(field), "{field} missing from the payload");
        }
    }

    #[test]
    fn a_downloads_file_from_an_older_build_still_loads() {
        // Fields this build added must not be required to parse, and the
        // fields it dropped since must not make the parse fail either.
        let json = r#"[{
            "id":"x","url":"https://e/f.zip","kind":"file","out_dir":"/tmp",
            "title":"f.zip","thumbnail":"","qualities":[],"quality":"",
            "percent":50.0,"speed":"","eta":"","error_msg":"","status":"paused"
        }]"#;
        let infos: Vec<DownloadInfo> = serde_json::from_str(json).unwrap();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].eta_secs, -1, "eta must default to unknown, not 0");
        assert!(infos[0].headers.is_empty());
    }

    // ---- settings --------------------------------------------------------

    #[test]
    fn out_of_range_settings_are_clamped() {
        let mut s = Settings { connections: 999, max_active: 0, server_port: 80, ..Default::default() };
        s.sane();
        assert_eq!(s.connections, MAX_CONNECTIONS);
        assert_eq!(s.max_active, 1);
        assert_eq!(s.server_port, server::DEFAULT_PORT, "privileged ports are not ours to bind");
        assert_eq!(s.lang, "fr");
    }

    #[test]
    fn defaults_leave_the_browser_listener_on_and_unpaired() {
        let s = Settings::default();
        assert!(s.server_enabled);
        assert!(s.token.is_empty(), "a token must be minted by pairing, never shipped");
        assert!(!s.autostart);
    }

    // ---- tokens ----------------------------------------------------------

    #[test]
    fn tokens_are_random_and_long_enough() {
        let a = random_hex(16);
        let b = random_hex(16);
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ---- the queue -------------------------------------------------------

    fn item(id: &str, status: &str) -> (String, Item) {
        let info = DownloadInfo { id: id.into(), status: status.into(), ..DownloadInfo::blank() };
        (id.to_string(), Item { info, cancel: None, saved_percent: 0.0 })
    }

    #[test]
    fn only_running_transfers_count_against_the_limit() {
        let items: HashMap<String, Item> = [
            item("a", "downloading"),
            item("b", "queued"),
            item("c", "paused"),
            item("d", "downloading"),
            item("e", "done"),
        ]
        .into_iter()
        .collect();
        assert_eq!(active_count(&items), 2);
    }

    // ---- secrets on disk -------------------------------------------------

    #[cfg(unix)]
    #[test]
    fn a_file_holding_a_secret_is_written_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("tg-priv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");

        // A file an earlier build left world-readable must be repaired, not
        // just written into — hence the pre-existing 0644.
        std::fs::write(&path, b"{}").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        write_private(&path, br#"{"token":"s3cret"}"#).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the pairing token would be readable by other accounts");
        assert_eq!(std::fs::read(&path).unwrap(), br#"{"token":"s3cret"}"#);
        std::fs::remove_dir_all(&dir).ok();
    }
}
