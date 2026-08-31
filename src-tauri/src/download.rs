// The transfer engine: everything between a URL and a finished file.
//
// One shape only — a plain HTTP GET, streamed to `<name>.part` and renamed on
// completion — but with the two things a browser's own downloader doesn't do:
// the file is split across parallel connections whenever the server honours
// byte ranges, and a pause resumes from the exact byte each connection stopped
// at, because per-chunk offsets are persisted next to the `.part`.
//
// Nothing here touches the app's state directly: progress goes out through the
// handful of `crate::` helpers at the top, which own the lock and the emit.

use std::collections::BTreeMap;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use crate::{log, now_secs, patch, set_title, update_file_progress};

// Multi-connection acceleration: split a range-capable download into parallel
// segments over separate HTTP connections (the IDM/aria2 trick — sidesteps the
// per-connection bandwidth cap many servers apply). Falls back to a single
// stream when the server doesn't advertise byte ranges or the size is small.
pub const DEFAULT_CONNECTIONS: u32 = 8;
pub const MAX_CONNECTIONS: u32 = 16;
const MIN_SEGMENTED: u64 = 4 * 1024 * 1024; // below 4 MiB, one connection is plenty
// Work-stealing granularity: the file is diced into many CHUNK-sized ranges and
// the workers pull the next range as soon as they free up. Small pieces keep
// every connection busy to the very end instead of stalling on one slow
// segment's tail (the equal-split failure mode).
const CHUNK: u64 = 4 * 1024 * 1024;
// Userspace write buffer per segment — coalesces the ~16 KiB reqwest chunks into
// larger, less frequent syscalls.
const WRITE_BUF: usize = 256 * 1024;
// Plenty of CDNs answer a request with no User-Agent at all with a 403, so send
// a browser-shaped one — the same trick IDM plays by borrowing the browser's.
// A download handed over by the extension arrives with the real browser's UA
// among its headers, and that one wins.
pub const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
// Transient network failures are routine on a long 8-connection download.
// Retry the affected piece instead of failing the whole file.
pub const MAX_RETRY: u32 = 5;

/// Headers a download carries, if any. A `BTreeMap` rather than a `HashMap`:
/// the order is then stable in `downloads.json`, so a resumed download diffs
/// cleanly and a bug report shows the same file twice in a row.
pub type Headers = BTreeMap<String, String>;

/// Headers the app must own rather than replay.
///
/// The extension sends the request headers the browser was about to use. Most
/// are exactly what makes the link work (Cookie, Referer, Authorization); a few
/// would break the transfer if replayed, because they describe *that* request
/// rather than this one: a `Range` would fight the segmenter, and an
/// `Accept-Encoding: gzip` would hand us a body whose length no longer matches
/// the Content-Length we size the file from.
const REFUSED_HEADERS: &[&str] = &[
    "range",
    "if-range",
    "accept-encoding",
    "connection",
    "host",
    "content-length",
    "proxy-connection",
    "keep-alive",
    "transfer-encoding",
    "te",
    "upgrade",
];

/// Drop the headers we must not replay, and normalise the names to lowercase.
pub fn usable_headers(h: &Headers) -> Headers {
    h.iter()
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .filter(|(k, v)| {
            !k.is_empty()
                && !v.is_empty()
                && !REFUSED_HEADERS.contains(&k.as_str())
                && k.chars().all(|c| c.is_ascii_graphic() && c != ':')
        })
        .collect()
}

/// Build the client every request for one download goes through.
///
/// The browser's headers are set as client-wide defaults rather than per
/// request, so all eight segment connections and every retry look identical to
/// the server. reqwest drops `Cookie`/`Authorization` itself when a redirect
/// crosses to another host, which is the behaviour we want anyway.
pub fn client_for(headers: &Headers) -> Result<reqwest::Client, String> {
    let mut map = reqwest::header::HeaderMap::new();
    for (k, v) in usable_headers(headers) {
        if let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(k.as_bytes()),
            reqwest::header::HeaderValue::from_str(&v),
        ) {
            map.insert(name, value);
        }
    }
    let has_ua = map.contains_key(reqwest::header::USER_AGENT);
    let mut b = reqwest::Client::builder().default_headers(map);
    if !has_ua {
        b = b.user_agent(UA);
    }
    b.build().map_err(|e| e.to_string())
}

// ---- failure vocabulary ---------------------------------------------------

/// Reduce any failure to one of a handful of codes the UI can phrase properly.
///
/// Nothing raw ever reaches a card. reqwest reports a single DNS failure as two
/// hundred characters naming the host, the port, the errno and the error that
/// wrapped it; that is noise to everyone who is simply offline. The original
/// text is not lost — the caller stashes it in `error_detail`, which the card
/// offers to copy.
///
/// Only ever applied where the message is about to be *shown*, never inside the
/// retry loop: `is_fatal` treats a leading `@` as "do not retry", and a dropped
/// connection is exactly what deserves retrying.
pub fn classify(msg: &str, fallback: &str) -> String {
    // Already a code from our own code paths (@link_expired, @no_range…).
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
    if has(&["http 401", "http 403", "login required", "forbidden"]) {
        return "@blocked".into();
    }
    if has(&["http 404", "404 not found", "no longer available", "does not exist"]) {
        return "@not_found".into();
    }
    if has(&["too many requests", "rate limit", "http 429"]) {
        return "@rate_limited".into();
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

pub fn backoff(attempt: u32) -> std::time::Duration {
    std::time::Duration::from_millis(300u64 << attempt.min(5))
}

/// A server verdict (HTTP status or a fixed `@code`) is not a hiccup — no retry.
pub fn is_fatal(err: &str) -> bool {
    err.starts_with("HTTP ") || err.starts_with('@')
}

/// Turn a refused response into an error message.
///
/// Signed/expiring links (S3 presigned, CloudFront, googlevideo, Mega…) die with
/// 401/403/410 once their deadline passes — the bytes on disk are still good, the
/// *address* is what went stale. Flag that case so the UI can ask for a fresh link
/// instead of presenting a dead end, which is what IDM's "refresh download
/// address" does.
pub fn http_err(status: reqwest::StatusCode) -> String {
    match status.as_u16() {
        401 | 403 | 410 => "@link_expired".into(),
        code => format!("HTTP {code}"),
    }
}

// ---- naming ---------------------------------------------------------------

/// Strip path separators so a server-supplied name can't escape the out dir.
///
/// The wider set (`* ? " < > |`) is rejected by NTFS but legal on ext4; we strip
/// it everywhere so the same download produces the same filename on every
/// platform. Trailing dots go too — Windows silently drops them, which would
/// leave the finished file under a different name than the `.part` we renamed.
pub fn sanitize(name: &str) -> String {
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

pub fn percent_decode(s: &str) -> String {
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
pub fn filename_from_url(url: &str) -> Option<String> {
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

pub fn parse_disposition_filename(cd: &str) -> Option<String> {
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

/// Split "archive.tar.gz" into ("archive.tar", ".gz") — stem and extension.
fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        // A leading dot is the whole name (".bashrc"), not an extension.
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// A name that is not already taken in `dir`: "video.mp4" -> "video (2).mp4".
///
/// Two downloads of the same file used to land on the same path, and the second
/// one silently overwrote the first the moment it finished. The `.part` counts
/// as taken too: a name whose transfer is still in flight is not free either.
pub fn unique_name(dir: &Path, name: &str) -> String {
    let taken = |n: &str| dir.join(n).exists() || dir.join(format!("{n}.part")).exists();
    if !taken(name) {
        return name.to_string();
    }
    let (stem, ext) = split_ext(name);
    for n in 2..10_000 {
        let candidate = format!("{stem} ({n}){ext}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    name.to_string()
}

// ---- probing --------------------------------------------------------------

/// What one probe of the URL established, before any bytes are committed.
pub struct Probe {
    pub total: Option<u64>,
    /// Only ever true when a `206` was actually observed — never on the strength
    /// of an `Accept-Ranges` header alone, which servers advertise and then ignore.
    pub ranges_ok: bool,
    /// ETag (preferred) or Last-Modified, replayed as `If-Range` on every resume
    /// so a file that changed server-side restarts instead of splicing two
    /// different versions into one output.
    pub validator: Option<String>,
    /// Post-redirect target. Every segment must request *this*, not the original:
    /// otherwise each of the workers re-resolves the redirect independently and
    /// signed/expiring CDN links diverge between connections.
    pub final_url: String,
    pub filename: Option<String>,
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

pub fn parse_content_range_total(v: &str) -> Option<u64> {
    v.rsplit('/').next()?.trim().parse().ok()
}

/// Establish size, range support, identity and final URL for an arbitrary link.
///
/// HEAD alone isn't enough in the wild: many servers answer it with 405, and
/// plenty omit `Accept-Ranges` on HEAD while honouring `Range` perfectly on GET.
/// So HEAD is treated as a hint, and a one-byte `Range: bytes=0-0` GET is the
/// authority — a `206` there proves both the size and that ranges really work.
pub async fn probe(client: &reqwest::Client, url: &str) -> Probe {
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

// ---- resume bookkeeping ---------------------------------------------------

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

pub fn meta_path(part: &Path) -> PathBuf {
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

/// How many slots the segment strip is squashed into, whatever the file size.
/// A 4 GB file is 1024 chunks; sending those raw would be ~10 KB of JSON four
/// times a second, and no progress bar can show more ticks than it has pixels.
const SEG_BUCKETS: usize = 48;

/// Average completion (0-100) of the chunks falling in each bucket.
pub fn bucket_segments(done: &[u64], total: u64, nchunks: usize) -> Vec<u8> {
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

/// The `.part` is only worth renaming when it holds the whole file.
///
/// Every path that ends a transfer goes through here. A short `.part` renamed
/// into place is the one failure a downloader must never produce: it looks
/// finished, opens broken, and the bytes that would have completed it are gone.
fn commit(part: &Path, final_path: &Path, expect: u64) -> Result<(), String> {
    if expect > 0 {
        let have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
        if have != expect {
            return Err("@short_read".into());
        }
    }
    std::fs::rename(part, final_path).map_err(|e| e.to_string())
}

// ---- the transfer ---------------------------------------------------------

/// Returns Ok(true) when the file completed, Ok(false) when paused/cancelled.
pub async fn run_file_download(
    app: &AppHandle,
    id: &str,
    connections: u32,
    cancel: Arc<AtomicBool>,
) -> Result<bool, String> {
    let (url, out_dir, mut title, headers) = {
        let state = crate::state(app);
        let items = state.items.lock().unwrap();
        let it = items.get(id).ok_or("unknown download")?;
        (
            it.info.url.clone(),
            it.info.out_dir.clone(),
            it.info.title.clone(),
            it.info.headers.clone(),
        )
    };
    if out_dir.is_empty() {
        return Err("@no_dir".into());
    }
    let dir = PathBuf::from(&out_dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let client = client_for(&headers)?;
    let p = probe(&client, &url).await;

    if title.is_empty() {
        let name = p
            .filename
            .clone()
            // Resolve the name off the *final* URL: a redirect to a CDN usually
            // carries the real filename the short link hid.
            .or_else(|| filename_from_url(&p.final_url))
            .or_else(|| filename_from_url(&url))
            .unwrap_or_else(|| "download".into());
        // Only ever computed once, on the first run: the name is persisted, so
        // a resume reuses it instead of stepping to "file (2)" and stranding
        // the bytes already on disk under the old name.
        title = unique_name(&dir, &name);
        set_title(app, id, &title);
    }
    let part = dir.join(format!("{title}.part"));
    let final_path = dir.join(&title);

    // The probe already knows the size, whether a pause can be resumed, and
    // where the file will land.
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

    let workers = connections.clamp(1, MAX_CONNECTIONS) as usize;
    match p.total {
        Some(t) if p.ranges_ok && t >= MIN_SEGMENTED && workers > 1 => {
            run_segmented(
                app,
                id,
                &client,
                &p.final_url,
                &part,
                &final_path,
                t,
                p.validator.as_deref(),
                workers,
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
    commit(part, final_path, total)?;
    Ok(true)
}

/// Parallel segments over `workers` connections, each writing its own slice of a
/// pre-sized `.part`. Resumable via the `.meta` sidecar.
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
    workers: usize,
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

    let workers = workers.min(nchunks).max(1);
    // A small file gets fewer chunks than the connection count, so "8
    // connections" would be a lie. Report what actually runs.
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
    commit(part, final_path, total)?;
    let _ = std::fs::remove_file(&meta);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn sanitize_drops_trailing_dots_and_control_chars() {
        // Windows silently eats trailing dots, which would leave the renamed
        // file under a different name than the one we tracked.
        assert_eq!(sanitize("report..."), "report");
        assert_eq!(sanitize("  spaced.mp4  "), "spaced.mp4");
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

    #[test]
    fn an_extension_survives_deduplication() {
        assert_eq!(split_ext("archive.tar.gz"), ("archive.tar", ".gz"));
        assert_eq!(split_ext("noext"), ("noext", ""));
        // A dotfile is a name, not an extension.
        assert_eq!(split_ext(".bashrc"), (".bashrc", ""));
    }

    #[test]
    fn a_taken_name_gets_a_counter_not_an_overwrite() {
        let dir = std::env::temp_dir().join(format!("turbograb-unique-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert_eq!(unique_name(&dir, "video.mp4"), "video.mp4");

        std::fs::write(dir.join("video.mp4"), b"x").unwrap();
        assert_eq!(unique_name(&dir, "video.mp4"), "video (2).mp4");

        // A transfer still in flight owns its name too: the `.part` counts.
        std::fs::write(dir.join("video (2).mp4.part"), b"x").unwrap();
        assert_eq!(unique_name(&dir, "video.mp4"), "video (3).mp4");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- headers from the browser ----------------------------------------

    #[test]
    fn headers_that_would_break_the_transfer_are_dropped() {
        let sent: Headers = [
            ("Cookie", "session=abc"),
            ("Referer", "https://example.test/page"),
            ("Range", "bytes=0-99"),
            ("Accept-Encoding", "gzip, deflate"),
            ("Host", "example.test"),
            ("", "empty-name"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

        let kept = usable_headers(&sent);
        assert_eq!(kept.get("cookie").map(String::as_str), Some("session=abc"));
        assert!(kept.contains_key("referer"));
        // A replayed Range would fight the segmenter; gzip would make
        // Content-Length disagree with the bytes we write.
        assert!(!kept.contains_key("range"));
        assert!(!kept.contains_key("accept-encoding"));
        assert!(!kept.contains_key("host"));
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn the_browsers_own_user_agent_wins_over_ours() {
        let mut h = Headers::new();
        h.insert("User-Agent".into(), "Firefox/1.0".into());
        // Building must not fail, and the header must survive normalisation:
        // a CDN that fingerprints the browser has to see the browser's UA.
        assert!(client_for(&h).is_ok());
        assert_eq!(usable_headers(&h).get("user-agent").map(String::as_str), Some("Firefox/1.0"));
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
    fn classify_maps_transport_failures_to_network() {
        assert_eq!(classify("error sending request for url (https://h/f)", "@x"), "@network");
        assert_eq!(classify("Connection refused (os error 111)", "@x"), "@network");
    }

    #[test]
    fn classify_passes_our_own_codes_through_and_falls_back_otherwise() {
        assert_eq!(classify("@link_expired", "@download_failed"), "@link_expired");
        // Raw text that matches no rule must never reach a card verbatim.
        assert_eq!(classify("weird internal thing", "@download_failed"), "@download_failed");
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

    // ---- the completion guard --------------------------------------------

    #[test]
    fn a_short_part_is_never_renamed_into_place() {
        let dir = std::env::temp_dir().join(format!("turbograb-commit-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let part = dir.join("f.bin.part");
        let final_path = dir.join("f.bin");

        std::fs::write(&part, b"12345").unwrap();
        // 5 bytes on disk, 10 expected: renaming here is the one failure a
        // downloader must never produce — a file that looks finished and isn't.
        assert_eq!(commit(&part, &final_path, 10), Err("@short_read".into()));
        assert!(!final_path.exists());

        assert!(commit(&part, &final_path, 5).is_ok());
        assert!(final_path.exists() && !part.exists());
        let _ = std::fs::remove_dir_all(&dir);
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
