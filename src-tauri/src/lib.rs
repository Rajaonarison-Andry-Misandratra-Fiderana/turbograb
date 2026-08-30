// YouTube downloader backend.
//
// Two-phase flow per item:
//   1. FETCH  — `yt-dlp -J` dumps metadata (title, thumbnail, available
//               formats) WITHOUT downloading. Item shows as "fetching".
//               On failure -> "error" with a message. On success -> "ready"
//               with a list of quality options.
//   2. DOWNLOAD — user picks a quality, `start_download` spawns `yt-dlp` with
//               the chosen format. Progress streams to the UI.
//
// Pause = kill the child (partial `.part` kept); resume = respawn with `-c`.
// State is persisted to the app-data dir so a crash mid-download leaves an
// "interrupted" record that can be resumed from the `.part` file.

use std::collections::{HashMap, VecDeque};
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures_util::StreamExt;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

// Tauri strips the `binaries/` dir and the target-triple suffix, copying the
// sidecar to <exe_dir>/yt-dlp. `.sidecar()` resolves the name verbatim against
// <exe_dir>, so it must be the bare basename — "binaries/yt-dlp" would look for
// <exe_dir>/binaries/yt-dlp and fail with "No such file or directory".
const SIDECAR: &str = "yt-dlp";

// Try several YouTube player clients: `default` gives the full resolution list
// on a normal (residential) connection; `tv`/`android` are fallbacks that dodge
// the "confirm you're not a bot" check when the default client is blocked.
const EXTRACTOR_ARGS: &str = "youtube:player_client=default,tv,android";

#[derive(Clone, Default, Serialize, Deserialize)]
struct Quality {
    label: String, // "1080p", "320 kbps"
    value: String, // "1080", "320" (empty = best available)
    /// Estimated bytes, 0 = unknown. Raw rather than pre-formatted: the size
    /// used to be humanized here in French, so the English UI still read "Mo".
    #[serde(default)]
    size_bytes: u64,
}

// Every field added after the first release carries `#[serde(default)]`: a
// `downloads.json` written by an older build must still load, otherwise the
// user's whole history vanishes on upgrade.
#[derive(Clone, Default, Serialize, Deserialize)]
struct DownloadInfo {
    id: String,
    url: String,
    kind: String, // "video" | "audio" | "file"
    out_dir: String,
    title: String,
    thumbnail: String,
    qualities: Vec<Quality>,
    quality: String, // chosen quality value
    percent: f32,
    speed: String,
    eta: String,
    error_msg: String,
    // "fetching" | "ready" | "downloading" | "paused"
    // | "interrupted" | "done" | "error"
    status: String,

    // ---- detail the UI shows; all optional on disk ----
    /// Size of the finished file. 0 when the server never said.
    #[serde(default)]
    total_bytes: u64,
    /// Bytes on disk right now. Survives a pause, so a resumed card can show
    /// "412 Mo / 1,2 Go" instead of a bare percentage.
    #[serde(default)]
    downloaded_bytes: u64,
    /// Parallel connections actually in flight (1 = plain single stream).
    #[serde(default)]
    connections: u32,
    /// Server honoured a Range request. False means a pause restarts from zero,
    /// which the user deserves to know *before* pausing.
    #[serde(default)]
    resumable: bool,
    /// Raw transfer rate in bytes/s, 0 = idle. `speed` above is a pre-formatted
    /// string with French units baked in; this one the UI can localize.
    #[serde(default)]
    speed_bps: f64,
    /// Seconds remaining, -1 = unknown. Same reason as `speed_bps`.
    #[serde(default = "unknown_eta")]
    eta_secs: i64,
    /// Current attempt of `retry_max` while recovering a dropped connection.
    /// 0 = not retrying. Without this a backoff looks like a frozen download.
    #[serde(default)]
    retry: u32,
    #[serde(default)]
    retry_max: u32,
    /// Completion per bucket, 0-100, at most `SEG_BUCKETS` entries — the shape
    /// of the transfer across its parallel connections. Live only: cleared
    /// before every write to disk (see `save`).
    ///
    /// Always serialized, empty included. `skip_serializing_if` was tried here
    /// to save a few bytes and made the field vanish from the payload whenever
    /// it was empty — which is every yt-dlp download — so the UI read
    /// `undefined.length` and the render threw. The wire shape must not depend
    /// on the value.
    #[serde(default)]
    segments: Vec<u8>,
    /// When the item entered the list. The list sorts on this, so a card never
    /// jumps position just because its status changed.
    #[serde(default)]
    created_at: u64,
    /// The untouched failure text behind `error_msg`. Never rendered; offered
    /// as "copy details" so a bug report can still carry the real diagnostic.
    #[serde(default)]
    error_detail: String,
    /// Absolute path of the finished file — what "open" and "delete" act on.
    /// For merged/extracted yt-dlp output this is the post-merge name, which
    /// `title` alone cannot give (see `handle_line`).
    #[serde(default)]
    file_path: String,
    /// Unix seconds. 0 = unknown (item predates the field, or never started).
    #[serde(default)]
    started_at: u64,
    #[serde(default)]
    finished_at: u64,
    /// Media length in seconds, from the yt-dlp metadata. 0 = unknown.
    #[serde(default)]
    duration: u64,
    /// Channel / uploader name, from the yt-dlp metadata.
    #[serde(default)]
    uploader: String,
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
            ..Default::default()
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

struct Item {
    info: DownloadInfo,
    child: Option<CommandChild>, // Some while a yt-dlp process (fetch/download) runs
    // For direct-file downloads there is no child process — the transfer runs in
    // an async task that polls this flag. Setting it true = pause/cancel request.
    cancel: Option<Arc<AtomicBool>>,
    saved_percent: f32,
}

/// Everything the app remembers between launches that is not a download.
///
/// Lives next to `downloads.json` rather than in the webview's localStorage:
/// the destination folder used to be forgotten on every launch while the
/// language persisted, which is exactly the kind of split the user noticed.
/// Rust also needs `lang` for the tray menu, which no localStorage can reach.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    /// Empty on first run; `load_settings` fills it from the OS Downloads dir.
    out_dir: String,
    lang: String,  // "fr" | "en"
    theme: String, // "system" | "light" | "dark"
    /// One-time hint that closing the window only hides it to the tray.
    tray_hint_shown: bool,
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
struct LogEntry {
    ts: u64,
    /// "error" | "warn" | "info"
    level: String,
    /// Which part spoke: "fetch", "download", "link", "app".
    source: String,
    msg: String,
    /// Download this line belongs to, empty when it is app-wide.
    id: String,
}

#[derive(Default)]
struct AppState {
    items: Mutex<HashMap<String, Item>>,
    settings: Mutex<Settings>,
    log: Mutex<VecDeque<LogEntry>>,
}

fn log(app: &AppHandle, level: &str, source: &str, id: &str, msg: impl Into<String>) {
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

fn state_file(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("downloads.json"))
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
    // Live-only: a 64-entry array per item, rewritten several times a second,
    // has no business in the on-disk history.
    for i in &mut infos {
        i.segments.clear();
    }
    if let Ok(json) = serde_json::to_vec_pretty(&infos) {
        let _ = std::fs::write(path, json);
    }
}

fn load(app: &AppHandle) {
    let Some(path) = state_file(app) else { return };
    let Ok(bytes) = std::fs::read(path) else { return };
    let Ok(infos) = serde_json::from_slice::<Vec<DownloadInfo>>(&bytes) else { return };

    let state = app.state::<AppState>();
    let mut items = state.items.lock().unwrap();
    for mut info in infos {
        match info.status.as_str() {
            "downloading" => {
                info.status = "interrupted".into();
                info.speed.clear();
                info.eta.clear();
                info.speed_bps = 0.0;
                info.eta_secs = -1;
                info.retry = 0;
                info.connections = 0;
            }
            "fetching" => {
                info.status = "error".into();
                // Fixed messages are i18n codes (leading '@'); the UI localizes
                // them. Dynamic yt-dlp errors pass through verbatim.
                info.error_msg = "@fetch_interrupted".into();
            }
            _ => {}
        }
        if info.retry_max == 0 {
            info.retry_max = MAX_RETRY;
        }
        if info.created_at == 0 {
            info.created_at = info.started_at;
        }
        items.insert(
            info.id.clone(),
            Item {
                saved_percent: info.percent,
                info,
                child: None,
                cancel: None,
            },
        );
    }
}

fn settings_file(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("settings.json"))
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
    if s.lang.is_empty() {
        s.lang = "fr".into();
    }
    if s.theme.is_empty() {
        s.theme = "system".into();
    }
    *app.state::<AppState>().settings.lock().unwrap() = s;
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn set_settings(app: AppHandle, state: State<AppState>, settings: Settings) -> Result<(), String> {
    let lang_changed = {
        let mut cur = state.settings.lock().unwrap();
        let changed = cur.lang != settings.lang;
        *cur = settings;
        changed
    };
    if let Some(path) = settings_file(&app) {
        let json = {
            let cur = state.settings.lock().unwrap();
            serde_json::to_vec_pretty(&*cur).map_err(|e| e.to_string())?
        };
        // Write-then-rename: a crash mid-write must not leave a truncated file
        // that resets every preference on the next launch.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    }
    if lang_changed {
        retray(&app)?;
    }
    Ok(())
}

// ---- phase 1: fetch metadata ----------------------------------------------

/// Pick the highest-quality thumbnail from the yt-dlp JSON.
fn best_thumbnail(v: &Value) -> String {
    if let Some(t) = v["thumbnail"].as_str() {
        return t.to_string();
    }
    if let Some(arr) = v["thumbnails"].as_array() {
        if let Some(last) = arr.last() {
            if let Some(u) = last["url"].as_str() {
                return u.to_string();
            }
        }
    }
    String::new()
}

/// A format's byte size — exact `filesize`, else yt-dlp's `filesize_approx`.
fn size_of(f: &Value) -> f64 {
    f["filesize"]
        .as_f64()
        .or_else(|| f["filesize_approx"].as_f64())
        .unwrap_or(0.0)
}

/// Build the quality options offered to the user for this item's kind, each with
/// an estimated output size.
fn build_qualities(v: &Value, kind: &str) -> Vec<Quality> {
    let duration = v["duration"].as_f64().unwrap_or(0.0);

    if kind == "audio" {
        // Re-encoded to mp3 at the target bitrate: size ≈ bitrate × duration.
        return ["320", "192", "128"]
            .iter()
            .map(|b| {
                let kbps: f64 = b.parse().unwrap_or(0.0);
                Quality {
                    label: format!("{b} kbps"),
                    value: b.to_string(),
                    size_bytes: (kbps * 1000.0 / 8.0 * duration).max(0.0) as u64,
                }
            })
            .collect();
    }

    let fmts = v["formats"].as_array();
    // Video is muxed with the best audio track — add that to each height's size.
    let best_audio = fmts
        .map(|arr| {
            arr.iter()
                .filter(|f| {
                    f["vcodec"].as_str().unwrap_or("none") == "none"
                        && f["acodec"].as_str().unwrap_or("none") != "none"
                })
                .map(size_of)
                .fold(0.0_f64, f64::max)
        })
        .unwrap_or(0.0);

    // Largest video stream per height (proxy for the highest-bitrate variant).
    let mut sizes: HashMap<i64, f64> = HashMap::new();
    if let Some(arr) = fmts {
        for f in arr {
            if f["vcodec"].as_str().unwrap_or("none") == "none" {
                continue;
            }
            if let Some(h) = f["height"].as_i64() {
                if h > 0 {
                    let s = size_of(f);
                    let e = sizes.entry(h).or_insert(0.0);
                    if s > *e {
                        *e = s;
                    }
                }
            }
        }
    }

    let mut heights: Vec<i64> = sizes.keys().copied().collect();
    heights.sort_unstable();
    heights.reverse();
    let mut out: Vec<Quality> = heights
        .into_iter()
        .map(|h| {
            let vid = sizes.get(&h).copied().unwrap_or(0.0);
            Quality {
                label: format!("{h}p"),
                value: h.to_string(),
                size_bytes: if vid > 0.0 { (vid + best_audio) as u64 } else { 0 },
            }
        })
        .collect();
    if out.is_empty() {
        // Empty value = "best available"; the UI supplies the localized label.
        out.push(Quality::default());
    }
    out
}

fn spawn_fetch(app: &AppHandle, id: &str, url: &str) -> Result<(), String> {
    let (mut rx, child) = app
        .shell()
        .sidecar(SIDECAR)
        .map_err(|e| e.to_string())?
        .args([
            "-J",
            "--no-playlist",
            "--no-warnings",
            "--extractor-args",
            EXTRACTOR_ARGS,
            url,
        ])
        .spawn()
        .map_err(|e| e.to_string())?;

    {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        if let Some(it) = items.get_mut(id) {
            it.child = Some(child);
        }
    }

    let app = app.clone();
    let id = id.to_string();
    tauri::async_runtime::spawn(async move {
        let mut out = String::new();
        let mut err = String::new();
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(b) => out.push_str(&String::from_utf8_lossy(&b)),
                CommandEvent::Stderr(b) => err.push_str(&String::from_utf8_lossy(&b)),
                CommandEvent::Terminated(p) => {
                    finish_fetch(&app, &id, p.code == Some(0), &out, &err);
                    break;
                }
                _ => {}
            }
        }
    });
    Ok(())
}

/// Reduce any failure to one of a handful of codes the UI can phrase properly.
///
/// Nothing raw ever reaches a card. yt-dlp and reqwest report a single DNS
/// failure as two hundred characters naming the host, the port, the errno and
/// the Python exception that wrapped it; that is noise to everyone who is
/// simply offline. The original text is not lost — the caller stashes it in
/// `error_detail`, which the card offers to copy.
///
/// Only ever applied where the message is about to be *shown*, never inside the
/// retry loop: `is_fatal` treats a leading `@` as "do not retry", and a dropped
/// connection is exactly what deserves retrying.
fn classify(msg: &str, fallback: &str) -> String {
    // Already a code from our own code paths (@link_expired, @no_ffmpeg…).
    if msg.starts_with('@') {
        return msg.to_string();
    }
    let low = msg.to_ascii_lowercase();
    let has = |ps: &[&str]| ps.iter().any(|p| low.contains(p));

    if has(&[
        "failed to resolve",
        "name resolution",
        "name or service not known",
        "nodename nor servname",
        "connection refused",
        "connection reset",
        "connection aborted",
        "network is unreachable",
        "timed out",
        "dns error",
        "error sending request",
    ]) {
        return "@network".into();
    }
    if has(&[
        "sign in to confirm",
        "not a bot",
        "age-restricted",
        "members-only",
        "private video",
        "not available in your country",
        "geo restricted",
        "geo-restricted",
        "login required",
    ]) {
        return "@blocked".into();
    }
    if has(&[
        "video unavailable",
        "no longer available",
        "has been removed",
        "does not exist",
        "http 404",
        "404 not found",
    ]) {
        return "@not_found".into();
    }
    if has(&["too many requests", "rate limit", "http 429"]) {
        return "@rate_limited".into();
    }
    if has(&[
        "unsupported url",
        "no video formats",
        "unable to extract",
        "no suitable format",
    ]) {
        return "@unsupported".into();
    }
    // Any 5xx, however it was phrased.
    if low.contains("http 5")
        || has(&[
            "internal server error",
            "bad gateway",
            "service unavailable",
            "gateway timeout",
        ])
    {
        return "@server".into();
    }
    if has(&["no space left", "disk full"]) {
        return "@no_space".into();
    }
    if has(&["permission denied", "read-only file system"]) {
        return "@no_write".into();
    }
    fallback.to_string()
}

fn last_error_line(stderr: &str) -> String {
    let err = stderr
        .lines()
        .rev()
        .find(|l| l.contains("ERROR"))
        .or_else(|| stderr.lines().rev().find(|l| !l.trim().is_empty()));
    match err {
        Some(l) => l.trim().trim_start_matches("ERROR:").trim().to_string(),
        None => "@analyze_failed".into(),
    }
}

fn finish_fetch(app: &AppHandle, id: &str, ok: bool, stdout: &str, stderr: &str) {
    let mut logged: Option<String> = None;
    {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        let Some(it) = items.get_mut(id) else { return };
        it.child = None;

        if !ok {
            it.info.status = "error".into();
            let raw = last_error_line(stderr);
            it.info.error_msg = classify(&raw, "@analyze_failed");
            it.info.error_detail =
                if it.info.error_msg == raw { String::new() } else { raw.clone() };
            logged = Some(raw);
        } else {
            match serde_json::from_str::<Value>(stdout) {
                Ok(v) => {
                    it.info.title = v["title"]
                        .as_str()
                        .unwrap_or(&it.info.url)
                        .to_string();
                    it.info.thumbnail = best_thumbnail(&v);
                    // Already parsed and, until now, discarded. Duration and
                    // channel are what tells two same-titled results apart.
                    it.info.duration = v["duration"].as_f64().unwrap_or(0.0).max(0.0) as u64;
                    it.info.uploader = v["uploader"]
                        .as_str()
                        .or_else(|| v["channel"].as_str())
                        .unwrap_or("")
                        .to_string();
                    it.info.qualities = build_qualities(&v, &it.info.kind);
                    it.info.quality = it
                        .info
                        .qualities
                        .first()
                        .map(|q| q.value.clone())
                        .unwrap_or_default();
                    it.info.status = "ready".into();
                }
                Err(_) => {
                    it.info.status = "error".into();
                    it.info.error_msg = "@unreadable".into();
                }
            }
        }
        let _ = app.emit("download-update", it.info.clone());
    }
    if let Some(raw) = logged {
        log(app, "error", "fetch", id, raw);
    }
    save(app);
}

// ---- phase 2: download ----------------------------------------------------

/// Does this ffmpeg actually start? `exists()` is not enough: a sidecar copied
/// from a system build stops running the moment the host bumps the libav*
/// sonames out from under it (ffmpeg 8 -> 9 moves libavdevice .so.62 -> .so.63),
/// and yt-dlp then dies with an opaque error deep into the download.
fn ffmpeg_runs(exe: &Path) -> bool {
    std::process::Command::new(exe)
        .arg("-version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Directory handed to yt-dlp via `--ffmpeg-location`.
///
/// Prefers the bundled sidecar next to our own executable, but only if it runs;
/// otherwise falls back to the first working `ffmpeg` on PATH. `None` means no
/// usable ffmpeg anywhere — the caller turns that into a clear UI error instead
/// of letting yt-dlp fail cryptically halfway through.
fn ffmpeg_location(_app: &AppHandle) -> Option<String> {
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf))
    {
        if ffmpeg_runs(&dir.join(exe_name("ffmpeg"))) {
            return Some(dir.to_string_lossy().into_owned());
        }
    }

    // PATH fallback. yt-dlp wants the *directory*, and expects ffprobe beside
    // ffmpeg, so only accept an entry that holds both.
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let ffmpeg = dir.join(exe_name("ffmpeg"));
        if dir.join(exe_name("ffprobe")).exists() && ffmpeg_runs(&ffmpeg) {
            Some(dir.to_string_lossy().into_owned())
        } else {
            None
        }
    })
}

fn build_args(info: &DownloadInfo, ffmpeg_dir: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--newline".into(),
        // `--print` below puts yt-dlp in quiet mode, which silences the
        // progress lines the UI lives on. `--progress` forces them back.
        // Verified: without it, zero PROG| lines reach us.
        "--progress".into(),
        "--no-playlist".into(),
        // Download fragments (DASH/HLS) in parallel — real speedup on the
        // segmented formats YouTube serves.
        "--concurrent-fragments".into(),
        CONNECTIONS.to_string(),
        "-c".into(), // continue partial files = resume
        "-o".into(),
        format!("{}/%(title)s.%(ext)s", info.out_dir),
        "--progress-template".into(),
        // Bytes as well as the pre-formatted strings: the UI shows
        // "412 Mo / 1,2 Go", which a percentage alone can't produce.
        concat!(
            "PROG|%(progress._percent_str)s|%(progress._speed_str)s",
            "|%(progress._eta_str)s|%(progress.downloaded_bytes)s",
            "|%(progress.total_bytes,progress.total_bytes_estimate)s",
            "|%(progress.speed)s|%(progress.eta)s"
        )
        .into(),
        // The only reliable source for the *final* filename. `[download]
        // Destination:` names the pre-merge stream (`Foo.f251.webm`), so a
        // merged video or an extracted mp3 leaves it pointing at a file that
        // no longer exists — which is why deleting used to miss the file.
        // `after_move` fires once the post-processors are done.
        "--print".into(),
        "after_move:FILE|%(filepath)s".into(),
        // `--print` implies `--simulate` for pre-download fields; be explicit
        // so a future template change can't silently stop downloading.
        "--no-simulate".into(),
        "--extractor-args".into(),
        EXTRACTOR_ARGS.into(),
    ];
    if let Some(dir) = ffmpeg_dir {
        args.push("--ffmpeg-location".into());
        args.push(dir.into());
    }
    match info.kind.as_str() {
        "audio" => {
            args.extend(["-x".into(), "--audio-format".into(), "mp3".into()]);
            if !info.quality.is_empty() {
                args.push("--audio-quality".into());
                args.push(format!("{}K", info.quality));
            }
        }
        _ => {
            let fmt = if info.quality.is_empty() {
                "bv*+ba/b".to_string()
            } else {
                let h = &info.quality;
                format!("bv*[height<={h}]+ba/b[height<={h}]")
            };
            args.extend(["-f".into(), fmt, "--merge-output-format".into(), "mp4".into()]);
        }
    }
    args.push(info.url.clone());
    args
}

fn spawn_download(app: &AppHandle, id: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let info = {
        let items = state.items.lock().unwrap();
        items.get(id).ok_or("unknown download")?.info.clone()
    };

    // Both paths need ffmpeg: audio runs `-x --audio-format mp3`, video merges
    // the separate bv*+ba streams. Without it yt-dlp would either fail late or
    // silently hand back a low-res progressive stream, so stop here and say so.
    let ffmpeg = ffmpeg_location(app);
    if ffmpeg.is_none() {
        let mut items = state.items.lock().unwrap();
        if let Some(it) = items.get_mut(id) {
            it.info.status = "error".into();
            it.info.error_msg = "@no_ffmpeg".into();
            let _ = app.emit("download-update", it.info.clone());
        }
        log(app, "error", "app", id, "ffmpeg sidecar not found next to the binary");
        return Err("@no_ffmpeg".into());
    }
    let args = build_args(&info, ffmpeg.as_deref());
    patch(app, id, |i| {
        i.connections = CONNECTIONS as u32;
        i.resumable = true; // yt-dlp runs with -c
        i.retry = 0;
        if i.started_at == 0 {
            i.started_at = now_secs();
        }
    });

    let (mut rx, child) = app
        .shell()
        .sidecar(SIDECAR)
        .map_err(|e| e.to_string())?
        .args(args)
        .spawn()
        .map_err(|e| e.to_string())?;

    {
        let mut items = state.items.lock().unwrap();
        if let Some(it) = items.get_mut(id) {
            it.child = Some(child);
            it.info.status = "downloading".into();
            it.info.error_msg.clear();
            let _ = app.emit("download-update", it.info.clone());
        }
    }
    save(app);

    let app = app.clone();
    let id = id.to_string();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(bytes) => {
                    handle_line(&app, &id, String::from_utf8_lossy(&bytes).trim());
                }
                CommandEvent::Terminated(payload) => {
                    finalize(&app, &id, payload.code == Some(0));
                    break;
                }
                _ => {}
            }
        }
    });
    Ok(())
}

/// Last path component, for both separators — yt-dlp echoes Windows paths with
/// backslashes, which a plain `rsplit('/')` would hand back whole.
fn base_name(path: &str) -> Option<String> {
    let name = path.rsplit(['/', '\\']).next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn handle_line(app: &AppHandle, id: &str, line: &str) {
    let mut should_save = false;
    {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        let Some(it) = items.get_mut(id) else { return };

        if let Some(rest) = line.strip_prefix("PROG|") {
            let mut parts = rest.split('|');
            if let Some(p) = parts.next() {
                if let Ok(v) = p.trim().trim_end_matches('%').parse::<f32>() {
                    it.info.percent = v;
                }
            }
            it.info.speed = parts.next().unwrap_or("").trim().to_string();
            it.info.eta = parts.next().unwrap_or("").trim().to_string();
            // yt-dlp prints "NA" for a field it doesn't have; parse failure
            // leaves the previous value, which is the honest thing to show.
            if let Ok(v) = parts.next().unwrap_or("").trim().parse::<u64>() {
                it.info.downloaded_bytes = v;
            }
            if let Ok(v) = parts.next().unwrap_or("").trim().parse::<u64>() {
                it.info.total_bytes = v;
            }
            // Raw rate and remaining seconds, so the UI can print them in the
            // user's own units instead of yt-dlp's English strings.
            it.info.speed_bps = parts
                .next()
                .unwrap_or("")
                .trim()
                .parse::<f64>()
                .unwrap_or(0.0)
                .max(0.0);
            it.info.eta_secs = parts
                .next()
                .unwrap_or("")
                .trim()
                .parse::<f64>()
                .map(|v| v as i64)
                .unwrap_or(-1);
            if it.info.percent - it.saved_percent >= 2.0 {
                should_save = true;
            }
        } else if let Some(path) = line.strip_prefix("FILE|") {
            let path = path.trim();
            it.info.file_path = path.to_string();
            if let Some(name) = base_name(path) {
                it.info.title = name;
            }
            should_save = true;
        } else if let Some(dest) = line.strip_prefix("[download] Destination:") {
            // Provisional: names the stream being fetched, and for a merge or an
            // audio extraction it is *not* the file left on disk. `FILE|` above
            // corrects it once yt-dlp has moved the finished file into place.
            if let Some(name) = base_name(dest.trim()) {
                it.info.title = name;
            }
        } else {
            return;
        }
        let _ = app.emit("download-update", it.info.clone());
    }
    if should_save {
        save(app);
    }
}

fn finalize(app: &AppHandle, id: &str, ok: bool) {
    let mut failed = false;
    {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        let Some(it) = items.get_mut(id) else { return };
        it.child = None;
        if it.info.status != "paused" {
            if ok {
                it.info.status = "done".into();
                it.info.percent = 100.0;
                it.info.finished_at = now_secs();
                it.info.speed.clear();
                it.info.eta.clear();
                it.info.speed_bps = 0.0;
                it.info.eta_secs = -1;
                it.info.connections = 0;
                if it.info.total_bytes > 0 {
                    it.info.downloaded_bytes = it.info.total_bytes;
                }
            } else {
                it.info.status = "error".into();
                it.info.error_msg = "@download_failed".into();
                it.info.error_detail.clear();
                failed = true;
            }
        }
        let _ = app.emit("download-update", it.info.clone());
    }
    if failed {
        log(app, "error", "download", id, "yt-dlp exited with a failure status");
    }
    save(app);
}

// ---- direct file downloads (the "Fichier" tab) ----------------------------
//
// No yt-dlp here — a plain HTTP GET streamed to `<name>.part`, renamed to the
// final name on completion. Pause/resume works via an HTTP Range request from
// the current `.part` size, so a resume continues instead of restarting.

/// Strip path separators so a server-supplied name can't escape the out dir.
///
/// The wider set (`* ? " < > |`) is rejected by NTFS but legal on ext4; we strip
/// it everywhere so the same download produces the same filename on every
/// platform. Trailing dots go too — Windows silently drops them, which would
/// leave the finished file under a different name than the `.part` we renamed.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
                || (c as u32) < 0x20
            {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .trim_end_matches('.')
        .trim()
        .to_string()
}

/// Sidecars keep their `.exe` suffix on Windows; Tauri only strips the target
/// triple. Looking for a bare "ffmpeg" there finds nothing.
fn exe_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(n) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(n);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Last path segment of the URL, percent-decoded, query/fragment dropped.
fn filename_from_url(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let raw = path.rsplit('/').next().unwrap_or("");
    let name = sanitize(&percent_decode(raw));
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Prefer the server's Content-Disposition filename (handles RFC 5987 `filename*`).
fn filename_from_disposition(resp: &reqwest::Response) -> Option<String> {
    let cd = resp
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)?
        .to_str()
        .ok()?;
    parse_disposition_filename(cd)
}

fn parse_disposition_filename(cd: &str) -> Option<String> {
    if let Some(i) = cd.find("filename*=") {
        let v = &cd[i + "filename*=".len()..];
        let v = v.split(';').next().unwrap_or(v).trim();
        let v = v.rsplit("''").next().unwrap_or(v); // drop the charset'' prefix
        let decoded = sanitize(&percent_decode(v));
        if !decoded.is_empty() {
            return Some(decoded);
        }
    }
    if let Some(i) = cd.find("filename=") {
        let v = &cd[i + "filename=".len()..];
        let v = v.split(';').next().unwrap_or(v).trim().trim_matches('"');
        let v = sanitize(v);
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
}

fn fmt_speed(bps: f64) -> String {
    let (v, u) = if bps >= 1e6 {
        (bps / 1e6, "Mo/s")
    } else if bps >= 1e3 {
        (bps / 1e3, "Ko/s")
    } else {
        (bps, "o/s")
    };
    format!("{v:.1} {u}")
}

fn fmt_eta(secs: f64) -> String {
    if !secs.is_finite() || secs <= 0.0 {
        return "--".into();
    }
    let s = secs as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// Mutate one item's info and push it to the UI. Every detail field added
/// after the fact goes through here, so there is one place that emits.
fn patch(app: &AppHandle, id: &str, f: impl FnOnce(&mut DownloadInfo)) {
    let state = app.state::<AppState>();
    let mut items = state.items.lock().unwrap();
    if let Some(it) = items.get_mut(id) {
        f(&mut it.info);
        let _ = app.emit("download-update", it.info.clone());
    }
}

fn set_title(app: &AppHandle, id: &str, title: &str) {
    {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        if let Some(it) = items.get_mut(id) {
            it.info.title = title.to_string();
            let _ = app.emit("download-update", it.info.clone());
        }
    }
    save(app);
}

/// How many slots the segment strip is squashed into, whatever the file size.
/// A 4 GB file is 1024 chunks; sending those raw would be ~10 KB of JSON four
/// times a second, and no progress bar can show more ticks than it has pixels.
const SEG_BUCKETS: usize = 48;

/// Average completion (0-100) of the chunks falling in each bucket.
fn bucket_segments(done: &[u64], total: u64, nchunks: usize) -> Vec<u8> {
    if nchunks == 0 || total == 0 {
        return Vec::new();
    }
    let buckets = nchunks.min(SEG_BUCKETS);
    (0..buckets)
        .map(|b| {
            let lo = b * nchunks / buckets;
            let hi = ((b + 1) * nchunks / buckets).max(lo + 1).min(nchunks);
            let (mut got, mut want) = (0u64, 0u64);
            for i in lo..hi {
                let cstart = i as u64 * CHUNK;
                let clen = ((i as u64 + 1) * CHUNK).min(total) - cstart;
                want += clen;
                got += done[i].min(clen);
            }
            if want == 0 { 0 } else { (got * 100 / want) as u8 }
        })
        .collect()
}

fn update_file_progress(
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
        it.info.speed = fmt_speed(speed);
        it.info.speed_bps = speed.max(0.0);
        it.info.segments = segments;
        let remaining = total.saturating_sub(downloaded) as f64;
        it.info.eta_secs = if speed > 1.0 && total > 0 {
            (remaining / speed) as i64
        } else {
            -1
        };
        it.info.eta = if speed > 1.0 {
            fmt_eta(remaining / speed)
        } else {
            "--".into()
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
        it.info.speed.clear();
        it.info.eta.clear();
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
            it.info.error_msg = classify(msg, "@download_failed");
            it.info.error_detail =
                if it.info.error_msg == msg { String::new() } else { msg.to_string() };
            reported = true;
            let _ = app.emit("download-update", it.info.clone());
        }
    }
    if reported {
        log(app, "error", "download", id, msg);
    }
    save(app);
}

// Multi-connection acceleration: split a range-capable download into parallel
// segments over separate HTTP connections (the IDM/aria2 trick — sidesteps the
// per-connection bandwidth cap many servers apply). Falls back to a single
// stream when the server doesn't advertise byte ranges or the size is small.
const CONNECTIONS: usize = 8;
const MIN_SEGMENTED: u64 = 4 * 1024 * 1024; // below 4 MiB, one connection is plenty
// Work-stealing granularity: the file is diced into many CHUNK-sized ranges and
// CONNECTIONS workers pull the next range as soon as they free up. Small pieces
// keep every connection busy to the very end instead of stalling on one slow
// segment's tail (the equal-split failure mode).
const CHUNK: u64 = 4 * 1024 * 1024;
// Userspace write buffer per segment — coalesces the ~16 KiB reqwest chunks into
// larger, less frequent syscalls.
const WRITE_BUF: usize = 256 * 1024;
// Plenty of CDNs answer a request with no User-Agent at all with a 403, so send
// a browser-shaped one — the same trick IDM plays by borrowing the browser's.
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
// Transient network failures are routine on a long 8-connection download.
// Retry the affected piece instead of failing the whole file.
const MAX_RETRY: u32 = 5;

fn backoff(attempt: u32) -> std::time::Duration {
    std::time::Duration::from_millis(300u64 << attempt.min(5))
}

/// A server verdict (HTTP status or a fixed `@code`) is not a hiccup — no retry.
fn is_fatal(err: &str) -> bool {
    err.starts_with("HTTP ") || err.starts_with('@')
}

/// Turn a refused response into an error message.
///
/// Signed/expiring links (S3 presigned, CloudFront, googlevideo, Mega…) die with
/// 401/403/410 once their deadline passes — the bytes on disk are still good, the
/// *address* is what went stale. Flag that case so the UI can ask for a fresh link
/// instead of presenting a dead end, which is what IDM's "refresh download
/// address" does.
fn http_err(status: reqwest::StatusCode) -> String {
    match status.as_u16() {
        401 | 403 | 410 => "@link_expired".into(),
        code => format!("HTTP {code}"),
    }
}

/// What one probe of the URL established, before any bytes are committed.
struct Probe {
    total: Option<u64>,
    /// Only ever true when a `206` was actually observed — never on the strength
    /// of an `Accept-Ranges` header alone, which servers advertise and then ignore.
    ranges_ok: bool,
    /// ETag (preferred) or Last-Modified, replayed as `If-Range` on every resume
    /// so a file that changed server-side restarts instead of splicing two
    /// different versions into one output.
    validator: Option<String>,
    /// Post-redirect target. Every segment must request *this*, not the original:
    /// otherwise each of the 8 workers re-resolves the redirect independently and
    /// signed/expiring CDN links diverge between connections.
    final_url: String,
    filename: Option<String>,
}

fn accepts_ranges(resp: &reqwest::Response) -> bool {
    resp.headers()
        .get(reqwest::header::ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("bytes"))
        .unwrap_or(false)
}

fn validator_of(resp: &reqwest::Response) -> Option<String> {
    let h = resp.headers();
    h.get(reqwest::header::ETAG)
        .or_else(|| h.get(reqwest::header::LAST_MODIFIED))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Total size out of `Content-Range: bytes 0-0/734003200`. A `*` total (server
/// doesn't know) fails to parse and correctly yields None.
fn content_range_total(resp: &reqwest::Response) -> Option<u64> {
    let v = resp
        .headers()
        .get(reqwest::header::CONTENT_RANGE)?
        .to_str()
        .ok()?;
    parse_content_range_total(v)
}

fn parse_content_range_total(v: &str) -> Option<u64> {
    v.rsplit('/').next()?.trim().parse().ok()
}

/// Establish size, range support, identity and final URL for an arbitrary link.
///
/// HEAD alone isn't enough in the wild: many servers answer it with 405, and
/// plenty omit `Accept-Ranges` on HEAD while honouring `Range` perfectly on GET.
/// So HEAD is treated as a hint, and a one-byte `Range: bytes=0-0` GET is the
/// authority — a `206` there proves both the size and that ranges really work.
async fn probe(client: &reqwest::Client, url: &str) -> Probe {
    let mut p = Probe {
        total: None,
        ranges_ok: false,
        validator: None,
        final_url: url.to_string(),
        filename: None,
    };

    if let Ok(r) = client.head(url).send().await {
        if r.status().is_success() {
            p.final_url = r.url().to_string();
            p.total = r.content_length();
            p.ranges_ok = accepts_ranges(&r);
            p.validator = validator_of(&r);
            p.filename = filename_from_disposition(&r);
        }
    }

    if let Ok(r) = client
        .get(&p.final_url)
        .header(reqwest::header::RANGE, "bytes=0-0")
        .send()
        .await
    {
        if r.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            p.final_url = r.url().to_string();
            p.validator = validator_of(&r).or(p.validator);
            p.filename = filename_from_disposition(&r).or(p.filename);
            if let Some(t) = content_range_total(&r) {
                p.total = Some(t);
                p.ranges_ok = true;
            }
        } else if r.status().is_success() {
            // Range ignored: a 200 here means one connection only, whatever the
            // Accept-Ranges header claimed.
            p.final_url = r.url().to_string();
            p.ranges_ok = false;
            p.total = r.content_length().or(p.total);
            p.validator = validator_of(&r).or(p.validator);
            p.filename = filename_from_disposition(&r).or(p.filename);
        }
    }
    p
}

/// Per-segment completed byte counts, persisted next to the `.part` so a paused
/// segmented download resumes each connection from where it stopped.
#[derive(Serialize, Deserialize)]
struct SegMeta {
    total: u64,
    /// The `If-Range` validator the bytes on disk were fetched against. If the
    /// server now reports a different one, those bytes belong to another version
    /// of the file and must be thrown away rather than resumed onto.
    #[serde(default)]
    validator: Option<String>,
    done: Vec<u64>,
}

fn meta_path(part: &Path) -> PathBuf {
    let mut s = part.as_os_str().to_os_string();
    s.push(".meta");
    PathBuf::from(s)
}

fn load_meta(path: &Path, total: u64, chunks: usize, validator: Option<&str>) -> Vec<u64> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<SegMeta>(&b).ok())
        .filter(|m| {
            m.total == total
                && m.done.len() == chunks
                // Same size is not the same file. When either side has a
                // validator they must agree; only when neither exists do we
                // fall back to trusting the size alone.
                && m.validator.as_deref() == validator
        })
        .map(|m| m.done)
        .unwrap_or_else(|| vec![0; chunks])
}

fn save_meta(path: &Path, total: u64, validator: Option<&str>, done: &[u64]) {
    if let Ok(json) = serde_json::to_vec(&SegMeta {
        total,
        validator: validator.map(|s| s.to_string()),
        done: done.to_vec(),
    }) {
        let _ = std::fs::write(path, json);
    }
}

/// Returns Ok(true) when the file completed, Ok(false) when paused/cancelled.
async fn run_file_download(
    app: &AppHandle,
    id: &str,
    cancel: Arc<AtomicBool>,
) -> Result<bool, String> {
    let (url, out_dir, mut title) = {
        let state = app.state::<AppState>();
        let items = state.items.lock().unwrap();
        let it = items.get(id).ok_or("unknown download")?;
        (
            it.info.url.clone(),
            it.info.out_dir.clone(),
            it.info.title.clone(),
        )
    };

    let client = reqwest::Client::builder()
        .user_agent(UA)
        .build()
        .map_err(|e| e.to_string())?;

    let p = probe(&client, &url).await;

    if title.is_empty() {
        title = p
            .filename
            .clone()
            // Resolve the name off the *final* URL: a redirect to a CDN usually
            // carries the real filename the short link hid.
            .or_else(|| filename_from_url(&p.final_url))
            .or_else(|| filename_from_url(&url))
            .unwrap_or_else(|| "fichier".into());
        set_title(app, id, &title); // persist so a later resume reuses this name
    }
    let part = PathBuf::from(&out_dir).join(format!("{title}.part"));
    let final_path = PathBuf::from(&out_dir).join(&title);

    // The probe already knows the size, whether a pause can be resumed, and
    // where the file will land. All three were being thrown away.
    let total_known = p.total.unwrap_or(0);
    let path_str = final_path.to_string_lossy().into_owned();
    patch(app, id, |i| {
        i.total_bytes = total_known;
        i.resumable = p.ranges_ok;
        i.file_path = path_str;
        i.retry = 0;
        if i.started_at == 0 {
            i.started_at = now_secs();
        }
    });

    match p.total {
        Some(t) if p.ranges_ok && t >= MIN_SEGMENTED => {
            run_segmented(
                app,
                id,
                &client,
                &p.final_url,
                &part,
                &final_path,
                t,
                p.validator.as_deref(),
                cancel,
            )
            .await
        }
        // One connection, retried as a whole: run_single resumes from the
        // `.part` length, so each retry continues rather than restarting.
        _ => {
            patch(app, id, |i| i.connections = 1);
            let mut attempt = 0u32;
            loop {
                match run_single(
                    app,
                    id,
                    &client,
                    &p.final_url,
                    &part,
                    &final_path,
                    p.validator.as_deref(),
                    cancel.clone(),
                )
                .await
                {
                    Ok(v) => return Ok(v),
                    Err(e) => {
                        attempt += 1;
                        if is_fatal(&e) || attempt > MAX_RETRY || cancel.load(Ordering::Relaxed) {
                            return Err(e);
                        }
                        // Surface the backoff: without it the card sits at the
                        // same percentage for seconds and looks frozen.
                        patch(app, id, |i| i.retry = attempt);
                        log(app, "warn", "download", id,
                            format!("retry {attempt}/{MAX_RETRY} after: {e}"));
                        tokio::time::sleep(backoff(attempt)).await;
                    }
                }
            }
        }
    }
}

/// Single connection. Resumes from the current `.part` size via a Range request.
// Plumbing: the arguments are all distinct values threaded down from
// run_file_download. Bundling them into a struct would only move the list.
#[allow(clippy::too_many_arguments)]
async fn run_single(
    app: &AppHandle,
    id: &str,
    client: &reqwest::Client,
    url: &str,
    part: &Path,
    final_path: &Path,
    validator: Option<&str>,
    cancel: Arc<AtomicBool>,
) -> Result<bool, String> {
    let existing = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    let mut req = client.get(url);
    if existing > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={existing}-"));
        // If-Range makes the server decide: unchanged file -> 206 and we append;
        // changed file -> 200 with the full body and we truncate. Without it a
        // server honours the Range regardless and we'd splice two versions.
        if let Some(v) = validator {
            req = req.header(reqwest::header::IF_RANGE, v);
        }
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(http_err(resp.status()));
    }
    let resumed = existing > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    // content_length() is the remaining bytes on a 206 — add what's on disk.
    let total = resp
        .content_length()
        .map(|l| if resumed { l + existing } else { l })
        .unwrap_or(0);

    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(part)
        .await
        .map_err(|e| e.to_string())?;
    let mut file = tokio::io::BufWriter::with_capacity(WRITE_BUF, file);

    let mut downloaded = if resumed { existing } else { 0 };
    let mut stream = resp.bytes_stream();
    let mut last = Instant::now();
    let mut last_bytes = downloaded;

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            // Flush buffered bytes so the `.part` length matches what resume reads.
            file.flush().await.map_err(|e| e.to_string())?;
            return Ok(false); // keep the `.part` so resume can continue
        }
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f64();
        if dt >= 0.25 {
            let speed = (downloaded - last_bytes) as f64 / dt;
            update_file_progress(app, id, downloaded, total, speed, Vec::new());
            last = now;
            last_bytes = downloaded;
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(part, final_path).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Parallel segments over `CONNECTIONS` connections, each writing its own slice
/// of a pre-sized `.part`. Resumable via the `.meta` sidecar.
#[allow(clippy::too_many_arguments)]
async fn run_segmented(
    app: &AppHandle,
    id: &str,
    client: &reqwest::Client,
    url: &str,
    part: &Path,
    final_path: &Path,
    total: u64,
    validator: Option<&str>,
    cancel: Arc<AtomicBool>,
) -> Result<bool, String> {
    // Dice the file into fixed CHUNK-sized ranges. `done[i]` tracks bytes already
    // on disk for chunk i (0 = untouched) so a paused download resumes each piece.
    let nchunks = total.div_ceil(CHUNK) as usize;
    let meta = meta_path(part);
    let done0 = load_meta(&meta, total, nchunks, validator);
    let validator = validator.map(|s| s.to_string());

    // Pre-size the target so each worker can seek to a chunk's offset and write.
    let need_create = std::fs::metadata(part).map(|m| m.len() != total).unwrap_or(true);
    if need_create {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            // Not truncate(true): set_len below sizes the file either way, and a
            // resumed download reaching this branch still holds bytes the `.meta`
            // may vouch for.
            .truncate(false)
            .open(part)
            .map_err(|e| e.to_string())?;
        f.set_len(total).map_err(|e| e.to_string())?;
    }

    let downloaded = Arc::new(AtomicU64::new(done0.iter().sum()));
    let done: Arc<Vec<AtomicU64>> =
        Arc::new(done0.iter().map(|&d| AtomicU64::new(d)).collect());
    // Work-stealing cursor: each worker grabs the next unclaimed chunk index.
    let next = Arc::new(AtomicUsize::new(0));
    // Lock-free UI throttle: `last_emit` holds nanos-since-start of the last emit,
    // `last_bytes` the total at that point. Whoever wins the CAS emits — no Mutex
    // in the per-chunk hot path.
    let start = Instant::now();
    let last_emit = Arc::new(AtomicU64::new(0));
    let last_bytes = Arc::new(AtomicU64::new(downloaded.load(Ordering::Relaxed)));

    let workers = CONNECTIONS.min(nchunks).max(1);
    // A small file gets fewer chunks than CONNECTIONS, so "8 connections" would
    // be a lie. Report what actually runs.
    patch(app, id, |i| i.connections = workers as u32);
    let mut tasks = Vec::with_capacity(workers);
    for _ in 0..workers {
        let client = client.clone();
        let url = url.to_string();
        let part = part.to_path_buf();
        let meta = meta.clone();
        let validator = validator.clone();
        let cancel = cancel.clone();
        let downloaded = downloaded.clone();
        let done = done.clone();
        let next = next.clone();
        let last_emit = last_emit.clone();
        let last_bytes = last_bytes.clone();
        let app = app.clone();
        let id = id.to_string();

        // One async block reused across workers => one future type, so join_all
        // over the Vec type-checks without boxing.
        tasks.push(async move {
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Ok::<(), String>(());
                }
                // Claim the next chunk. Past the end => this worker is finished.
                let idx = next.fetch_add(1, Ordering::Relaxed);
                if idx >= nchunks {
                    return Ok(());
                }
                let cstart = idx as u64 * CHUNK;
                let cend = ((idx as u64 + 1) * CHUNK).min(total); // exclusive
                let clen = cend - cstart;
                let already = done[idx].load(Ordering::Relaxed);
                if already >= clen {
                    continue; // resumed chunk already complete
                }

                // `seg_done` counts bytes handed to the buffer; `done[idx]` is only
                // advanced after a flush, so the persisted meta never claims bytes
                // that haven't reached the OS yet (safe resume).
                let mut seg_done = already;
                let mut attempt = 0u32;
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        done[idx].store(seg_done, Ordering::Relaxed);
                        return Ok(());
                    }
                    // A stream can tear on its very last byte, leaving nothing to
                    // ask for. Retrying then would send `bytes={cend}-{cend-1}`,
                    // an invalid range the server answers with 416 — which we'd
                    // report as a hard failure on an already-complete chunk.
                    if seg_done >= clen {
                        done[idx].store(clen, Ordering::Relaxed);
                        break;
                    }
                    let before = seg_done;
                    let from = cstart + seg_done;
                    let to = cend - 1; // Range end is inclusive

                    let mut req = client
                        .get(&url)
                        .header(reqwest::header::RANGE, format!("bytes={from}-{to}"));
                    if let Some(v) = &validator {
                        req = req.header(reqwest::header::IF_RANGE, v.as_str());
                    }
                    let resp = match req.send().await {
                        Ok(r) => r,
                        Err(e) => {
                            attempt += 1;
                            if attempt > MAX_RETRY {
                                return Err(e.to_string());
                            }
                            tokio::time::sleep(backoff(attempt)).await;
                            continue;
                        }
                    };
                    // Only a 206 is safe here. A 200 means the server ignored the
                    // Range (or If-Range saw the file change) and is sending the
                    // WHOLE file — writing that at `from` would splice a full copy
                    // into this one chunk's slot and silently corrupt the output.
                    if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                        return Err(if resp.status().is_success() {
                            "@no_range".into()
                        } else {
                            http_err(resp.status())
                        });
                    }

                    let file = tokio::fs::OpenOptions::new()
                        .write(true)
                        .open(&part)
                        .await
                        .map_err(|e| e.to_string())?;
                    let mut file = tokio::io::BufWriter::with_capacity(WRITE_BUF, file);
                    file.seek(SeekFrom::Start(from)).await.map_err(|e| e.to_string())?;

                    let mut stream = resp.bytes_stream();
                    let mut torn = false;
                    while let Some(chunk) = stream.next().await {
                        if cancel.load(Ordering::Relaxed) {
                            file.flush().await.map_err(|e| e.to_string())?;
                            done[idx].store(seg_done, Ordering::Relaxed);
                            return Ok(());
                        }
                        let chunk = match chunk {
                            Ok(c) => c,
                            Err(e) => {
                                // Land what we have so the retry restarts from a
                                // byte count that's genuinely on disk.
                                file.flush().await.map_err(|e| e.to_string())?;
                                done[idx].store(seg_done, Ordering::Relaxed);
                                attempt += 1;
                                if attempt > MAX_RETRY {
                                    return Err(e.to_string());
                                }
                                torn = true;
                                break;
                            }
                        };
                        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                        let n = chunk.len() as u64;
                        seg_done += n;
                        let tot = downloaded.fetch_add(n, Ordering::Relaxed) + n;

                        // Throttle: only the CAS winner emits + persists, ~4x/sec total.
                        let now = start.elapsed().as_nanos() as u64;
                        let prev = last_emit.load(Ordering::Relaxed);
                        if now.saturating_sub(prev) >= 250_000_000
                            && last_emit
                                .compare_exchange(prev, now, Ordering::Relaxed, Ordering::Relaxed)
                                .is_ok()
                        {
                            // Flush our buffer before recording progress on disk so meta
                            // stays <= actual bytes written.
                            file.flush().await.map_err(|e| e.to_string())?;
                            done[idx].store(seg_done, Ordering::Relaxed);
                            let dt = (now - prev) as f64 / 1e9;
                            let prev_bytes = last_bytes.swap(tot, Ordering::Relaxed);
                            let speed = (tot.saturating_sub(prev_bytes)) as f64 / dt;
                            let snap: Vec<u64> =
                                done.iter().map(|a| a.load(Ordering::Relaxed)).collect();
                            let segs = bucket_segments(&snap, total, nchunks);
                            update_file_progress(&app, &id, tot, total, speed, segs);
                            save_meta(&meta, total, validator.as_deref(), &snap);
                        }
                    }
                    if torn {
                        // Progress before the tear means the connection was healthy
                        // for a while — don't let one long chunk burn its retries on
                        // unrelated hiccups spread over an hour.
                        if seg_done > before {
                            attempt = 0;
                        }
                        tokio::time::sleep(backoff(attempt + 1)).await;
                        continue;
                    }
                    file.flush().await.map_err(|e| e.to_string())?;
                    if seg_done < clen {
                        // The stream ended without an error but short of the range
                        // we asked for. Marking the chunk done here would leave a
                        // hole in the middle of the file and still rename it into
                        // place as if it were whole.
                        done[idx].store(seg_done, Ordering::Relaxed);
                        attempt += 1;
                        if attempt > MAX_RETRY {
                            return Err("@short_read".into());
                        }
                        tokio::time::sleep(backoff(attempt)).await;
                        continue;
                    }
                    done[idx].store(clen, Ordering::Relaxed); // chunk fully on disk
                    break;
                }
            }
        });
    }

    let results = futures_util::future::join_all(tasks).await;
    for r in &results {
        if let Err(e) = r {
            return Err(e.clone());
        }
    }

    let snap: Vec<u64> = done.iter().map(|a| a.load(Ordering::Relaxed)).collect();
    if cancel.load(Ordering::Relaxed) {
        // paused — keep offsets (and the identity they belong to) for resume
        save_meta(&meta, total, validator.as_deref(), &snap);
        return Ok(false);
    }
    std::fs::rename(part, final_path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&meta);
    Ok(true)
}

fn spawn_file_download(app: &AppHandle, id: &str) -> Result<(), String> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        let it = items.get_mut(id).ok_or("unknown download")?;
        it.cancel = Some(cancel.clone());
        it.info.status = "downloading".into();
        it.info.error_msg.clear();
        let _ = app.emit("download-update", it.info.clone());
    }
    save(app);

    let app = app.clone();
    let id = id.to_string();
    tauri::async_runtime::spawn(async move {
        match run_file_download(&app, &id, cancel).await {
            Ok(true) => finish_file(&app, &id),
            Ok(false) => {} // paused/cancelled — status already set by the command
            Err(e) => fail_file(&app, &id, &e),
        }
    });
    Ok(())
}

// ---- commands -------------------------------------------------------------

#[tauri::command]
fn fetch_info(
    app: AppHandle,
    state: State<AppState>,
    id: String,
    url: String,
    kind: String,
) -> Result<(), String> {
    {
        let mut items = state.items.lock().unwrap();
        let info = DownloadInfo {
            id: id.clone(),
            url: url.clone(),
            kind,
            status: "fetching".into(),
            ..DownloadInfo::blank()
        };
        // Emit right away so the "fetching" card shows the moment the user
        // clicks Récupérer — the UI renders only from download-update events.
        let _ = app.emit("download-update", info.clone());
        items.insert(
            id.clone(),
            Item {
                info,
                child: None,
                cancel: None,
                saved_percent: 0.0,
            },
        );
    }
    save(&app);
    spawn_fetch(&app, &id, &url)
}

/// Re-run metadata fetch for an item that errored during phase 1.
#[tauri::command]
fn retry_fetch(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    let url = {
        let mut items = state.items.lock().unwrap();
        let it = items.get_mut(&id).ok_or("unknown download")?;
        it.info.status = "fetching".into();
        it.info.error_msg.clear();
        let _ = app.emit("download-update", it.info.clone());
        it.info.url.clone()
    };
    spawn_fetch(&app, &id, &url)
}

#[tauri::command]
fn start_download(
    app: AppHandle,
    state: State<AppState>,
    id: String,
    quality: String,
    out_dir: String,
) -> Result<(), String> {
    {
        let mut items = state.items.lock().unwrap();
        let it = items.get_mut(&id).ok_or("unknown download")?;
        it.info.quality = quality;
        it.info.out_dir = out_dir;
        it.info.started_at = now_secs();
        it.info.finished_at = 0;
        it.info.error_msg.clear();
        it.info.error_detail.clear();
    }
    spawn_download(&app, &id)
}

/// Direct-file download (the "Fichier" tab). No metadata phase — starts streaming
/// immediately; the filename is resolved from the response headers or the URL.
#[tauri::command]
fn start_file_download(
    app: AppHandle,
    state: State<AppState>,
    id: String,
    url: String,
    out_dir: String,
) -> Result<(), String> {
    {
        let mut items = state.items.lock().unwrap();
        let info = DownloadInfo {
            id: id.clone(),
            url: url.clone(),
            kind: "file".into(),
            out_dir,
            status: "downloading".into(),
            started_at: now_secs(),
            ..DownloadInfo::blank()
        };
        let _ = app.emit("download-update", info.clone());
        items.insert(
            id.clone(),
            Item {
                info,
                child: None,
                cancel: None,
                saved_percent: 0.0,
            },
        );
    }
    save(&app);
    spawn_file_download(&app, &id)
}

#[tauri::command]
fn pause_download(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    {
        let mut items = state.items.lock().unwrap();
        let it = items.get_mut(&id).ok_or("unknown download")?;
        it.info.status = "paused".into();
        it.info.speed.clear();
        it.info.eta.clear();
        it.info.speed_bps = 0.0;
        it.info.eta_secs = -1;
        it.info.connections = 0;
        // File download: signal the streaming task. yt-dlp: kill the child.
        if let Some(flag) = &it.cancel {
            flag.store(true, Ordering::Relaxed);
        }
        if let Some(child) = it.child.take() {
            child.kill().map_err(|e| e.to_string())?;
        }
        let _ = app.emit("download-update", it.info.clone());
    }
    save(&app);
    Ok(())
}

fn respawn(app: &AppHandle, id: &str) -> Result<(), String> {
    let is_file = {
        let state = app.state::<AppState>();
        let items = state.items.lock().unwrap();
        items.get(id).map(|it| it.info.kind == "file").unwrap_or(false)
    };
    if is_file {
        spawn_file_download(app, id)
    } else {
        spawn_download(app, id)
    }
}

/// Point an existing file download at a fresh URL and restart it, keeping the
/// bytes already on disk. This is the answer to an expired signed link: the
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
    if url.is_empty() {
        return Err("@bad_link".into());
    }
    {
        let mut items = state.items.lock().unwrap();
        let it = items.get_mut(&id).ok_or("unknown download")?;
        if it.info.kind != "file" {
            return Err("@bad_link".into()); // yt-dlp items re-fetch on their own
        }
        // Swapping the URL under a live task would leave the old workers writing
        // one file's bytes into another file's `.part`. Pause first.
        if it.info.status == "downloading" {
            return Err("@busy".into());
        }
        it.info.url = url;
        it.info.error_msg.clear();
    }
    save(&app);
    spawn_file_download(&app, &id)
}

#[tauri::command]
fn resume_download(app: AppHandle, id: String) -> Result<(), String> {
    respawn(&app, &id)
}

#[tauri::command]
fn resume_all(app: AppHandle, state: State<AppState>) -> Result<Vec<String>, String> {
    let ids: Vec<String> = {
        let items = state.items.lock().unwrap();
        items
            .values()
            .filter(|it| it.info.status == "interrupted" || it.info.status == "paused")
            .map(|it| it.info.id.clone())
            .collect()
    };
    for id in &ids {
        let _ = respawn(&app, id);
    }
    Ok(ids)
}

/// Delete the finished file plus any leftover `.part` / `.meta` sidecars.
fn remove_files(info: &DownloadInfo) {
    if info.out_dir.is_empty() || info.title.is_empty() {
        return;
    }
    // `file_path` is authoritative when yt-dlp reported it; `out_dir + title`
    // is the fallback for direct files and for items saved by older builds.
    let base = if info.file_path.is_empty() {
        PathBuf::from(&info.out_dir).join(&info.title)
    } else {
        PathBuf::from(&info.file_path)
    };
    let _ = std::fs::remove_file(&base);
    let part = PathBuf::from(&info.out_dir).join(format!("{}.part", info.title));
    let _ = std::fs::remove_file(&part);
    let _ = std::fs::remove_file(meta_path(&part));
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
        if let Some(mut it) = items.remove(&id) {
            if let Some(flag) = &it.cancel {
                flag.store(true, Ordering::Relaxed);
            }
            if let Some(child) = it.child.take() {
                let _ = child.kill();
            }
            if delete_file {
                remove_files(&it.info);
            }
        }
    }
    let _ = app.emit("download-removed", id);
    save(&app);
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

// ---- system tray ----------------------------------------------------------

fn show_main(app: &AppHandle) {
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Must happen before the webview is created, so before the Builder runs.
    #[cfg(target_os = "linux")]
    avoid_dmabuf_crash();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            load_settings(app.handle());
            load(app.handle());
            build_tray(app.handle())?;

            // The window ships hidden and the frontend reveals it after its
            // first paint, so a borderless window never flashes the wrong
            // colour. If the frontend fails to boot, that would leave the app
            // running with nothing on screen — so show it anyway after a beat.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                if let Some(w) = handle.get_webview_window("main") {
                    if !w.is_visible().unwrap_or(true) {
                        let _ = w.show();
                    }
                }
            });
            Ok(())
        })
        // Closing the window hides it to the tray instead of quitting; use the
        // tray's "Quitter" to exit for real.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            fetch_info,
            retry_fetch,
            start_download,
            start_file_download,
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
        // when it happens to be empty reaches JS as `undefined`, and the first
        // property access on it throws during render — which is exactly what
        // `skip_serializing_if` on `segments` caused.
        let json = serde_json::to_value(DownloadInfo::blank()).unwrap();
        let obj = json.as_object().expect("serializes to an object");
        for field in [
            "id", "url", "kind", "out_dir", "title", "thumbnail", "qualities",
            "quality", "percent", "speed", "eta", "error_msg", "error_detail",
            "status", "total_bytes", "downloaded_bytes", "connections",
            "resumable", "speed_bps", "eta_secs", "retry", "retry_max",
            "segments", "file_path", "started_at", "finished_at", "created_at",
            "duration", "uploader",
        ] {
            assert!(obj.contains_key(field), "`{field}` missing from the payload");
        }
    }

    #[test]
    fn a_pre_existing_downloads_file_still_loads() {
        // Only the fields the first release wrote. Upgrading must not wipe a
        // user's history.
        let legacy = r#"{
            "id": "abc", "url": "https://example.com/x", "kind": "file",
            "out_dir": "/tmp", "title": "x", "thumbnail": "", "qualities": [],
            "quality": "", "percent": 12.5, "speed": "", "eta": "",
            "error_msg": "", "status": "paused"
        }"#;
        let info: DownloadInfo = serde_json::from_str(legacy).unwrap();
        assert_eq!(info.id, "abc");
        assert_eq!(info.percent, 12.5);
        assert!(info.segments.is_empty());
        assert_eq!(info.retry_max, 0); // load() repairs this
    }

    // ---- error classification -------------------------------------------

    #[test]
    fn classify_maps_transport_failures_to_network() {
        // The real shape of it: yt-dlp wraps a Python exception, reqwest nests
        // its own. Both must collapse to the same code.
        let ytdlp = "[youtube] K_o2ejRHLws: Unable to download API page: \
HTTPSConnection(host='www.youtube.com', port=443): Failed to resolve \
'www.youtube.com' ([Errno -3] Temporary failure in name resolution)";
        assert_eq!(classify(ytdlp, "@analyze_failed"), "@network");
        assert_eq!(
            classify("error sending request for url (…): dns error", "@download_failed"),
            "@network"
        );
        assert_eq!(classify("Connection reset by peer", "@download_failed"), "@network");
    }

    #[test]
    fn classify_covers_every_category() {
        let cases = [
            ("Sign in to confirm you're not a bot", "@blocked"),
            ("Video unavailable", "@not_found"),
            ("HTTP 404 Not Found", "@not_found"),
            ("HTTP 429 Too Many Requests", "@rate_limited"),
            ("Unsupported URL: https://example.com/x", "@unsupported"),
            ("HTTP 503", "@server"),
            ("Bad gateway", "@server"),
            ("No space left on device", "@no_space"),
            ("Permission denied (os error 13)", "@no_write"),
        ];
        for (raw, want) in cases {
            assert_eq!(classify(raw, "@download_failed"), want, "for {raw:?}");
        }
    }

    #[test]
    fn classify_passes_our_own_codes_through() {
        // A code from our own paths must survive untouched, or `is_fatal`
        // stops recognising it and a dead link would be retried five times.
        assert_eq!(classify("@link_expired", "@download_failed"), "@link_expired");
        assert_eq!(classify("@no_ffmpeg", "@analyze_failed"), "@no_ffmpeg");
    }

    #[test]
    fn classify_falls_back_rather_than_leaking_raw_text() {
        let weird = "yt-dlp said something nobody anticipated";
        assert_eq!(classify(weird, "@analyze_failed"), "@analyze_failed");
    }

    // ---- filenames --------------------------------------------------------

    #[test]
    fn sanitize_blocks_path_escapes() {
        assert_eq!(sanitize("../../etc/passwd"), ".._.._etc_passwd");
        assert_eq!(sanitize("a\\b:c"), "a_b_c");
    }

    #[test]
    fn sanitize_strips_chars_windows_rejects() {
        // Legal on ext4, fatal on NTFS. Stripped everywhere so the same URL
        // yields the same filename on every platform.
        assert_eq!(sanitize(r#"a*b?c"d<e>f|g"#), "a_b_c_d_e_f_g");
    }

    #[test]
    fn sanitize_drops_trailing_dots() {
        // Windows silently eats these, which would leave the renamed file under
        // a different name than the one we tracked.
        assert_eq!(sanitize("report..."), "report");
        assert_eq!(sanitize("  spaced.mp4  "), "spaced.mp4");
    }

    #[test]
    fn sanitize_drops_control_chars() {
        assert_eq!(sanitize("a\nb\tc"), "a_b_c");
    }

    #[test]
    fn filename_from_url_drops_query_and_decodes() {
        assert_eq!(
            filename_from_url("https://h/a/b/My%20File.mp4?sig=x&t=1#frag").as_deref(),
            Some("My File.mp4")
        );
        assert_eq!(filename_from_url("https://h/").as_deref(), None);
    }

    #[test]
    fn disposition_prefers_rfc5987_form() {
        assert_eq!(
            parse_disposition_filename("attachment; filename=\"fallback.bin\"; filename*=UTF-8''caf%C3%A9.mp4").as_deref(),
            Some("café.mp4")
        );
        assert_eq!(
            parse_disposition_filename("attachment; filename=\"plain.bin\"").as_deref(),
            Some("plain.bin")
        );
        assert_eq!(parse_disposition_filename("inline").as_deref(), None);
    }

    #[test]
    fn disposition_cannot_escape_the_out_dir() {
        assert_eq!(
            parse_disposition_filename("attachment; filename=\"../../evil.sh\"").as_deref(),
            Some(".._.._evil.sh")
        );
    }

    // ---- probe parsing ----------------------------------------------------

    #[test]
    fn content_range_yields_total() {
        assert_eq!(parse_content_range_total("bytes 0-0/734003200"), Some(734003200));
        assert_eq!(parse_content_range_total("bytes 5-9/10"), Some(10));
    }

    #[test]
    fn content_range_unknown_total_is_none() {
        // `*` means the server won't say. Must not be mistaken for a size.
        assert_eq!(parse_content_range_total("bytes 0-0/*"), None);
        assert_eq!(parse_content_range_total("garbage"), None);
    }

    // ---- error classification --------------------------------------------

    #[test]
    fn expired_link_statuses_are_flagged_for_relinking() {
        for code in [401u16, 403, 410] {
            assert_eq!(
                http_err(reqwest::StatusCode::from_u16(code).unwrap()),
                "@link_expired",
                "status {code} should ask the user for a fresh link"
            );
        }
    }

    #[test]
    fn other_statuses_stay_verbatim() {
        assert_eq!(http_err(reqwest::StatusCode::NOT_FOUND), "HTTP 404");
        assert_eq!(http_err(reqwest::StatusCode::from_u16(416).unwrap()), "HTTP 416");
    }

    #[test]
    fn server_verdicts_are_not_retried() {
        assert!(is_fatal("HTTP 404"));
        assert!(is_fatal("@link_expired"));
        assert!(is_fatal("@no_range"));
        // A transport error is a hiccup: retry it.
        assert!(!is_fatal("error sending request for url (...)"));
    }

    #[test]
    fn backoff_grows_then_caps() {
        let a = backoff(1);
        let b = backoff(3);
        assert!(b > a);
        // Capped so a long-lived download never sleeps for minutes.
        assert_eq!(backoff(5), backoff(9));
        assert!(backoff(9) <= std::time::Duration::from_secs(10));
    }

    #[test]
    fn sidecar_name_matches_the_platform() {
        let n = exe_name("ffmpeg");
        if cfg!(windows) {
            assert_eq!(n, "ffmpeg.exe");
        } else {
            assert_eq!(n, "ffmpeg");
        }
    }

    // ---- resume metadata --------------------------------------------------

    fn tmp_meta(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("turbograb-meta-test-{tag}"));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn meta_round_trips() {
        let p = tmp_meta("roundtrip");
        save_meta(&p, 1000, Some("\"etag1\""), &[10, 20, 30]);
        assert_eq!(load_meta(&p, 1000, 3, Some("\"etag1\"")), vec![10, 20, 30]);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn meta_rejected_when_the_file_changed_server_side() {
        // Same size, different ETag: the bytes on disk belong to another version
        // and resuming onto them would splice two files together.
        let p = tmp_meta("validator");
        save_meta(&p, 1000, Some("\"etag1\""), &[10, 20, 30]);
        assert_eq!(load_meta(&p, 1000, 3, Some("\"etag2\"")), vec![0, 0, 0]);
        // A server that stopped sending a validator is equally untrustworthy.
        assert_eq!(load_meta(&p, 1000, 3, None), vec![0, 0, 0]);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn meta_rejected_on_size_or_chunk_mismatch() {
        let p = tmp_meta("shape");
        save_meta(&p, 1000, Some("\"e\""), &[10, 20, 30]);
        assert_eq!(load_meta(&p, 999, 3, Some("\"e\"")), vec![0, 0, 0]);
        assert_eq!(load_meta(&p, 1000, 4, Some("\"e\"")), vec![0; 4]);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn missing_meta_starts_from_zero() {
        let p = tmp_meta("absent");
        assert_eq!(load_meta(&p, 500, 2, Some("\"e\"")), vec![0, 0]);
    }

    #[test]
    fn meta_resumes_on_size_alone_when_no_validator_exists() {
        // Documented fallback: a server offering neither ETag nor Last-Modified
        // leaves size as the only check we have.
        let p = tmp_meta("novalidator");
        save_meta(&p, 800, None, &[5, 5]);
        assert_eq!(load_meta(&p, 800, 2, None), vec![5, 5]);
        let _ = std::fs::remove_file(&p);
    }

    // ---- probe() against a throwaway server -------------------------------
    //
    // The server is deliberately hostile in the ways real ones are: it refuses
    // HEAD, it lies about Accept-Ranges, and it expires links. Hand-rolled over
    // a TcpListener so the test needs no extra crate and no external process.

    fn spawn_server() -> String {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut start = String::new();
                if reader.read_line(&mut start).is_err() || start.is_empty() {
                    continue;
                }
                let mut parts = start.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let path = parts.next().unwrap_or("/").to_string();

                let mut has_range = false;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if line.to_ascii_lowercase().starts_with("range:") {
                        has_range = true;
                    }
                }

                let resp: Vec<u8> = match path.as_str() {
                    // Refuses HEAD outright, but honours Range on GET. HEAD-only
                    // probing would wrongly conclude "no ranges, unknown size".
                    "/ok" => {
                        if method == "HEAD" {
                            b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
                        } else if has_range {
                            b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/1000\r\nContent-Length: 1\r\nAccept-Ranges: bytes\r\nETag: \"abc123\"\r\nContent-Disposition: attachment; filename=\"real.bin\"\r\nConnection: close\r\n\r\nX".to_vec()
                        } else {
                            b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\nX".to_vec()
                        }
                    }
                    // Advertises byte ranges and then ignores them: the trap that
                    // used to let 8 workers each write a full copy of the file.
                    "/norange" => {
                        let mut h = b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nAccept-Ranges: bytes\r\nETag: \"zzz\"\r\nConnection: close\r\n\r\n".to_vec();
                        if method != "HEAD" {
                            h.extend(std::iter::repeat_n(b'Y', 1000));
                        }
                        h
                    }
                    // A signed link past its deadline.
                    _ => b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                };
                let _ = stream.write_all(&resp);
                let _ = stream.flush();
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn probe_recovers_size_and_ranges_when_head_is_refused() {
        let base = spawn_server();
        let client = reqwest::Client::builder().user_agent(UA).build().unwrap();
        let p = probe(&client, &format!("{base}/ok")).await;

        assert_eq!(p.total, Some(1000), "size must come from Content-Range");
        assert!(p.ranges_ok, "a 206 proves ranges work even though HEAD 405'd");
        assert_eq!(p.validator.as_deref(), Some("\"abc123\""));
        assert_eq!(p.filename.as_deref(), Some("real.bin"));
    }

    #[tokio::test]
    async fn probe_refuses_to_segment_a_server_that_ignores_range() {
        let base = spawn_server();
        let client = reqwest::Client::builder().user_agent(UA).build().unwrap();
        let p = probe(&client, &format!("{base}/norange")).await;

        // Accept-Ranges said "bytes"; the 200 to our Range request says otherwise.
        // Trusting the header here is what corrupted the output.
        assert!(!p.ranges_ok, "a 200 to a Range request must disable segmenting");
        assert_eq!(p.total, Some(1000));
    }

    #[tokio::test]
    async fn probe_of_an_expired_link_yields_nothing_to_segment() {
        let base = spawn_server();
        let client = reqwest::Client::builder().user_agent(UA).build().unwrap();
        let p = probe(&client, &format!("{base}/gone")).await;

        // No size, no ranges -> run_single, which surfaces the 403 as @link_expired.
        assert_eq!(p.total, None);
        assert!(!p.ranges_ok);
    }
}
