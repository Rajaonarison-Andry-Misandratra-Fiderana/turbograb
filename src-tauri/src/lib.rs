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

use std::collections::HashMap;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
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

#[derive(Clone, Serialize, Deserialize)]
struct Quality {
    label: String, // "1080p", "320 kbps"
    value: String, // "1080", "320" (empty = best available)
    size: String,  // human estimate, "45.2 Mo" (empty = unknown)
}

#[derive(Clone, Serialize, Deserialize)]
struct DownloadInfo {
    id: String,
    url: String,
    kind: String, // "video" | "audio"
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
}

struct Item {
    info: DownloadInfo,
    child: Option<CommandChild>, // Some while a yt-dlp process (fetch/download) runs
    // For direct-file downloads there is no child process — the transfer runs in
    // an async task that polls this flag. Setting it true = pause/cancel request.
    cancel: Option<Arc<AtomicBool>>,
    saved_percent: f32,
}

#[derive(Default)]
struct AppState {
    items: Mutex<HashMap<String, Item>>,
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
    let infos: Vec<DownloadInfo> = {
        let mut items = state.items.lock().unwrap();
        for it in items.values_mut() {
            it.saved_percent = it.info.percent;
        }
        items.values().map(|it| it.info.clone()).collect()
    };
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
            }
            "fetching" => {
                info.status = "error".into();
                // Fixed messages are i18n codes (leading '@'); the UI localizes
                // them. Dynamic yt-dlp errors pass through verbatim.
                info.error_msg = "@fetch_interrupted".into();
            }
            _ => {}
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

/// Human-readable byte size, e.g. "45.2 Mo". Empty when unknown (<= 0).
fn human_size(bytes: f64) -> String {
    if bytes <= 0.0 {
        return String::new();
    }
    let (v, u) = if bytes >= 1e9 {
        (bytes / 1e9, "Go")
    } else if bytes >= 1e6 {
        (bytes / 1e6, "Mo")
    } else if bytes >= 1e3 {
        (bytes / 1e3, "Ko")
    } else {
        (bytes, "o")
    };
    format!("{v:.1} {u}")
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
                    size: human_size(kbps * 1000.0 / 8.0 * duration),
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
                size: human_size(if vid > 0.0 { vid + best_audio } else { 0.0 }),
            }
        })
        .collect();
    if out.is_empty() {
        out.push(Quality {
            label: "Meilleure".into(),
            value: String::new(),
            size: String::new(),
        });
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
    {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        let Some(it) = items.get_mut(id) else { return };
        it.child = None;

        if !ok {
            it.info.status = "error".into();
            it.info.error_msg = last_error_line(stderr);
        } else {
            match serde_json::from_str::<Value>(stdout) {
                Ok(v) => {
                    it.info.title = v["title"]
                        .as_str()
                        .unwrap_or(&it.info.url)
                        .to_string();
                    it.info.thumbnail = best_thumbnail(&v);
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
    save(app);
}

// ---- phase 2: download ----------------------------------------------------

fn ffmpeg_location(_app: &AppHandle) -> Option<String> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    if dir.join("ffmpeg").exists() {
        Some(dir.to_string_lossy().into_owned())
    } else {
        None
    }
}

fn build_args(info: &DownloadInfo, ffmpeg_dir: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--newline".into(),
        "--no-playlist".into(),
        // Download fragments (DASH/HLS) in parallel — real speedup on the
        // segmented formats YouTube serves.
        "--concurrent-fragments".into(),
        "4".into(),
        "-c".into(), // continue partial files = resume
        "-o".into(),
        format!("{}/%(title)s.%(ext)s", info.out_dir),
        "--progress-template".into(),
        "PROG|%(progress._percent_str)s|%(progress._speed_str)s|%(progress._eta_str)s".into(),
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

    let ffmpeg = ffmpeg_location(app);
    let args = build_args(&info, ffmpeg.as_deref());

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
            if it.info.percent - it.saved_percent >= 2.0 {
                should_save = true;
            }
        } else if let Some(dest) = line.strip_prefix("[download] Destination:") {
            if let Some(name) = dest.trim().rsplit('/').next() {
                it.info.title = name.to_string();
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
    {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        let Some(it) = items.get_mut(id) else { return };
        it.child = None;
        if it.info.status != "paused" {
            if ok {
                it.info.status = "done".into();
                it.info.percent = 100.0;
            } else {
                it.info.status = "error".into();
                it.info.error_msg = "@download_failed".into();
            }
        }
        let _ = app.emit("download-update", it.info.clone());
    }
    save(app);
}

// ---- direct file downloads (the "Fichier" tab) ----------------------------
//
// No yt-dlp here — a plain HTTP GET streamed to `<name>.part`, renamed to the
// final name on completion. Pause/resume works via an HTTP Range request from
// the current `.part` size, so a resume continues instead of restarting.

/// Strip path separators so a server-supplied name can't escape the out dir.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if matches!(c, '/' | '\\' | ':') { '_' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
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

fn update_file_progress(app: &AppHandle, id: &str, downloaded: u64, total: u64, speed: f64) {
    let mut should_save = false;
    {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        let Some(it) = items.get_mut(id) else { return };
        if total > 0 {
            it.info.percent = (downloaded as f32 / total as f32) * 100.0;
        }
        it.info.speed = fmt_speed(speed);
        let remaining = total.saturating_sub(downloaded) as f64;
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
        it.info.speed.clear();
        it.info.eta.clear();
        let _ = app.emit("download-update", it.info.clone());
    }
    save(app);
}

fn fail_file(app: &AppHandle, id: &str, msg: &str) {
    {
        let state = app.state::<AppState>();
        let mut items = state.items.lock().unwrap();
        let Some(it) = items.get_mut(id) else { return };
        it.cancel = None;
        // A pause races the stream: don't clobber "paused" with "error".
        if it.info.status != "paused" {
            it.info.status = "error".into();
            it.info.error_msg = msg.to_string();
            let _ = app.emit("download-update", it.info.clone());
        }
    }
    save(app);
}

// Multi-connection acceleration: split a range-capable download into parallel
// segments over separate HTTP connections (the IDM/aria2 trick — sidesteps the
// per-connection bandwidth cap many servers apply). Falls back to a single
// stream when the server doesn't advertise byte ranges or the size is small.
const CONNECTIONS: usize = 6;
const MIN_SEGMENTED: u64 = 4 * 1024 * 1024; // below 4 MiB, one connection is plenty

/// Per-segment completed byte counts, persisted next to the `.part` so a paused
/// segmented download resumes each connection from where it stopped.
#[derive(Serialize, Deserialize)]
struct SegMeta {
    total: u64,
    done: Vec<u64>,
}

fn meta_path(part: &Path) -> PathBuf {
    let mut s = part.as_os_str().to_os_string();
    s.push(".meta");
    PathBuf::from(s)
}

fn load_meta(path: &Path, total: u64, conn: usize) -> Vec<u64> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<SegMeta>(&b).ok())
        .filter(|m| m.total == total && m.done.len() == conn)
        .map(|m| m.done)
        .unwrap_or_else(|| vec![0; conn])
}

fn save_meta(path: &Path, total: u64, done: &[u64]) {
    if let Ok(json) = serde_json::to_vec(&SegMeta {
        total,
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
        .build()
        .map_err(|e| e.to_string())?;

    // One HEAD probe: total size + byte-range support (+ a filename if the
    // server sends Content-Disposition here).
    let head = client.head(&url).send().await.ok();
    let (total, ranges_ok) = match &head {
        Some(r) if r.status().is_success() => {
            let ranges = r
                .headers()
                .get(reqwest::header::ACCEPT_RANGES)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.contains("bytes"))
                .unwrap_or(false);
            (r.content_length(), ranges)
        }
        _ => (None, false),
    };

    if title.is_empty() {
        title = head
            .as_ref()
            .and_then(filename_from_disposition)
            .or_else(|| filename_from_url(&url))
            .unwrap_or_else(|| "fichier".into());
        set_title(app, id, &title); // persist so a later resume reuses this name
    }
    let part = PathBuf::from(&out_dir).join(format!("{title}.part"));
    let final_path = PathBuf::from(&out_dir).join(&title);

    match total {
        Some(t) if ranges_ok && t >= MIN_SEGMENTED => {
            run_segmented(app, id, &client, &url, &part, &final_path, t, cancel).await
        }
        _ => run_single(app, id, &client, &url, &part, &final_path, cancel).await,
    }
}

/// Single connection. Resumes from the current `.part` size via a Range request.
async fn run_single(
    app: &AppHandle,
    id: &str,
    client: &reqwest::Client,
    url: &str,
    part: &Path,
    final_path: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<bool, String> {
    let existing = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    let mut req = client.get(url);
    if existing > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let resumed = existing > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    // content_length() is the remaining bytes on a 206 — add what's on disk.
    let total = resp
        .content_length()
        .map(|l| if resumed { l + existing } else { l })
        .unwrap_or(0);

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(part)
        .map_err(|e| e.to_string())?;

    let mut downloaded = if resumed { existing } else { 0 };
    let mut stream = resp.bytes_stream();
    let mut last = Instant::now();
    let mut last_bytes = downloaded;

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Ok(false); // keep the `.part` so resume can continue
        }
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f64();
        if dt >= 0.25 {
            let speed = (downloaded - last_bytes) as f64 / dt;
            update_file_progress(app, id, downloaded, total, speed);
            last = now;
            last_bytes = downloaded;
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(part, final_path).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Parallel segments over `CONNECTIONS` connections, each writing its own slice
/// of a pre-sized `.part`. Resumable via the `.meta` sidecar.
async fn run_segmented(
    app: &AppHandle,
    id: &str,
    client: &reqwest::Client,
    url: &str,
    part: &Path,
    final_path: &Path,
    total: u64,
    cancel: Arc<AtomicBool>,
) -> Result<bool, String> {
    // Don't spawn more connections than there are megabytes to fetch.
    let conn = CONNECTIONS.min((total / (1024 * 1024)).max(1) as usize).max(1);
    let meta = meta_path(part);
    let done0 = load_meta(&meta, total, conn);

    // Pre-size the target so each segment can seek to its offset and write.
    let need_create = std::fs::metadata(part).map(|m| m.len() != total).unwrap_or(true);
    if need_create {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(part)
            .map_err(|e| e.to_string())?;
        f.set_len(total).map_err(|e| e.to_string())?;
    }

    let base = total / conn as u64;
    let downloaded = Arc::new(AtomicU64::new(done0.iter().sum()));
    let done: Arc<Vec<AtomicU64>> =
        Arc::new(done0.iter().map(|&d| AtomicU64::new(d)).collect());
    // (last emit time, bytes at last emit) — throttles UI updates across tasks.
    let progress = Arc::new(Mutex::new((Instant::now(), downloaded.load(Ordering::Relaxed))));

    let mut tasks = Vec::with_capacity(conn);
    for i in 0..conn {
        let start = i as u64 * base;
        let end = if i == conn - 1 { total } else { (i as u64 + 1) * base }; // exclusive
        let seg_len = end - start;
        let already = done0[i];

        let client = client.clone();
        let url = url.to_string();
        let part = part.to_path_buf();
        let meta = meta.clone();
        let cancel = cancel.clone();
        let downloaded = downloaded.clone();
        let done = done.clone();
        let progress = progress.clone();
        let app = app.clone();
        let id = id.to_string();

        // NB: a single async block reused across loop iterations => one future
        // type, so join_all over the Vec type-checks without boxing.
        tasks.push(async move {
            if already >= seg_len {
                return Ok::<(), String>(()); // this segment already finished
            }
            let from = start + already;
            let to = end - 1; // Range end is inclusive
            let resp = client
                .get(&url)
                .header(reqwest::header::RANGE, format!("bytes={from}-{to}"))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("HTTP {}", resp.status().as_u16()));
            }
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .open(&part)
                .map_err(|e| e.to_string())?;
            file.seek(SeekFrom::Start(from)).map_err(|e| e.to_string())?;

            let mut seg_done = already;
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                if cancel.load(Ordering::Relaxed) {
                    return Ok(()); // paused — offsets are persisted by the throttle below
                }
                let chunk = chunk.map_err(|e| e.to_string())?;
                file.write_all(&chunk).map_err(|e| e.to_string())?;
                let n = chunk.len() as u64;
                seg_done += n;
                done[i].store(seg_done, Ordering::Relaxed);
                let tot = downloaded.fetch_add(n, Ordering::Relaxed) + n;

                let mut p = progress.lock().unwrap();
                let dt = p.0.elapsed().as_secs_f64();
                if dt >= 0.25 {
                    let speed = (tot - p.1) as f64 / dt;
                    *p = (Instant::now(), tot);
                    drop(p);
                    update_file_progress(&app, &id, tot, total, speed);
                    let snap: Vec<u64> = done.iter().map(|a| a.load(Ordering::Relaxed)).collect();
                    save_meta(&meta, total, &snap);
                }
            }
            Ok(())
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
        save_meta(&meta, total, &snap); // paused — keep offsets for resume
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
            out_dir: String::new(),
            title: String::new(),
            thumbnail: String::new(),
            qualities: Vec::new(),
            quality: String::new(),
            percent: 0.0,
            speed: String::new(),
            eta: String::new(),
            error_msg: String::new(),
            status: "fetching".into(),
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
            title: String::new(),
            thumbnail: String::new(),
            qualities: Vec::new(),
            quality: String::new(),
            percent: 0.0,
            speed: String::new(),
            eta: String::new(),
            error_msg: String::new(),
            status: "downloading".into(),
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
    let base = PathBuf::from(&info.out_dir).join(&info.title);
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

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Afficher TurboGrab", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            load(app.handle());
            build_tray(app.handle())?;
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
            resume_download,
            resume_all,
            cancel_download,
            clear_finished,
            list_downloads,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
