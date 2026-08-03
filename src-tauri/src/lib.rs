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
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
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
                info.error_msg = "Analyse interrompue".into();
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

/// Build the quality options offered to the user for this item's kind.
fn build_qualities(v: &Value, kind: &str) -> Vec<Quality> {
    if kind == "audio" {
        return ["320", "192", "128"]
            .iter()
            .map(|b| Quality {
                label: format!("{b} kbps"),
                value: b.to_string(),
            })
            .collect();
    }
    let mut heights: Vec<i64> = Vec::new();
    if let Some(fmts) = v["formats"].as_array() {
        for f in fmts {
            if f["vcodec"].as_str().unwrap_or("none") == "none" {
                continue;
            }
            if let Some(h) = f["height"].as_i64() {
                if h > 0 {
                    heights.push(h);
                }
            }
        }
    }
    heights.sort_unstable();
    heights.dedup();
    heights.reverse();
    let mut out: Vec<Quality> = heights
        .into_iter()
        .map(|h| Quality {
            label: format!("{h}p"),
            value: h.to_string(),
        })
        .collect();
    if out.is_empty() {
        out.push(Quality {
            label: "Meilleure".into(),
            value: String::new(),
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
        None => "Impossible d'analyser cette URL".into(),
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
                    it.info.error_msg = "Réponse illisible de yt-dlp".into();
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
                it.info.error_msg = "Le téléchargement a échoué".into();
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

    // If a name was already chosen on a prior run, a `.part` may exist — resume
    // from its size.
    let mut part_path = if title.is_empty() {
        None
    } else {
        Some(PathBuf::from(&out_dir).join(format!("{title}.part")))
    };
    let existing: u64 = part_path
        .as_ref()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .unwrap_or(0);

    let mut req = client.get(&url);
    if existing > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }

    // First run: settle the filename now (Content-Disposition wins, else URL).
    if title.is_empty() {
        title = filename_from_disposition(&resp)
            .or_else(|| filename_from_url(&url))
            .unwrap_or_else(|| "fichier".into());
        part_path = Some(PathBuf::from(&out_dir).join(format!("{title}.part")));
        set_title(app, id, &title); // persist so a later resume reuses this name
    }
    let part_path = part_path.unwrap();
    let final_path = PathBuf::from(&out_dir).join(&title);

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
        .open(&part_path)
        .map_err(|e| e.to_string())?;

    let mut downloaded = if resumed { existing } else { 0 };
    let mut stream = resp.bytes_stream();
    let mut last = std::time::Instant::now();
    let mut last_bytes = downloaded;

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Ok(false); // keep the `.part` so resume can continue
        }
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        let now = std::time::Instant::now();
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
    std::fs::rename(&part_path, &final_path).map_err(|e| e.to_string())?;
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

#[tauri::command]
fn cancel_download(app: AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    {
        let mut items = state.items.lock().unwrap();
        if let Some(mut it) = items.remove(&id) {
            if let Some(flag) = &it.cancel {
                flag.store(true, Ordering::Relaxed);
            }
            if let Some(child) = it.child.take() {
                let _ = child.kill();
            }
        }
    }
    let _ = app.emit("download-removed", id);
    save(&app);
    Ok(())
}

#[tauri::command]
fn clear_finished(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    {
        let mut items = state.items.lock().unwrap();
        items.retain(|_, it| it.info.status != "done" && it.info.status != "error");
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            load(app.handle());
            Ok(())
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
