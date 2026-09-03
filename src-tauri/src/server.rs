// The loopback API the browser extension talks to.
//
// A browser extension cannot speak to a desktop app directly; it can only make
// HTTP requests. So TurboGrab listens on 127.0.0.1 and the extension POSTs the
// download it just intercepted — URL, suggested filename, and the request
// headers the browser would have used (cookies included, which is what makes a
// logged-in download work at all outside the browser).
//
// Written by hand rather than on a web framework: four routes, tiny JSON
// bodies, and one localhost listener do not justify pulling hyper/axum and
// their transitive tree into a download manager.
//
// SECURITY. The socket is bound to 127.0.0.1, so nothing off the machine can
// reach it — but every program and every web page on this machine can. Hence:
//   * /add and /downloads require a token, generated from OS entropy, stored in
//     settings.json and never sent anywhere by us;
//   * the only way to obtain that token is /pair, which asks *the user* in the
//     app window, naming who is asking. A page that guesses the port still gets
//     nothing without a click;
//   * /ping is deliberately unauthenticated and answers nothing but "running".

use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

use crate::download::Headers;
use crate::{now_secs, AppState};

pub const DEFAULT_PORT: u16 = 8787;

/// A pairing prompt expires rather than pinning a connection open forever.
const PAIR_TIMEOUT: Duration = Duration::from_secs(120);

/// How many pairing prompts may be open at once.
///
/// One. `/pair` is the only unauthenticated route that *does* something: it
/// raises the window and puts a dialog in front of the user. Anything running
/// on this machine can call it in a loop, and without a cap each call stacks
/// another prompt and steals focus again — the app becomes unusable, and a user
/// clicking to make the storm stop is exactly the accident that hands out a
/// token. Serialising the prompts means the answer to the first one is given
/// deliberately, and every request arriving behind it is refused without ever
/// reaching the screen.
const MAX_PENDING_PAIRS: usize = 1;

/// Requests are a URL plus a few headers. Anything larger is not ours.
const MAX_BODY: usize = 256 * 1024;

/// What the extension sends to hand a download over.
#[derive(Deserialize)]
struct AddRequest {
    url: String,
    #[serde(default)]
    filename: String,
    /// The request headers the browser was about to use. Cookie and Referer are
    /// the ones that matter: without them a session-gated link 403s the moment
    /// it leaves the browser.
    #[serde(default)]
    headers: Headers,
    /// Content-Length the browser saw, when it saw one. Only used to show a
    /// size before the probe answers.
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
struct PairRequest {
    #[serde(default)]
    client: String,
}

/// Start listening. Returns the sender that stops this listener; dropping it
/// (or sending on it) makes the accept loop exit at its next turn.
pub fn start(app: &AppHandle, port: u16) -> oneshot::Sender<()> {
    let (tx, rx) = oneshot::channel();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        serve(app, port, rx).await;
    });
    tx
}

async fn serve(app: AppHandle, port: u16, mut stop: oneshot::Receiver<()>) {
    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(l) => l,
        Err(e) => {
            // A busy port is a configuration problem the user can fix, not a
            // reason to take the app down with us.
            crate::log(&app, "error", "server", "", format!("port {port}: {e}"));
            let _ = app.emit("server-state", ServerState::down(port, &e.to_string()));
            return;
        }
    };
    crate::log(&app, "info", "server", "", format!("listening on 127.0.0.1:{port}"));
    let _ = app.emit("server-state", ServerState::up(port));

    loop {
        tokio::select! {
            _ = &mut stop => break,
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { continue };
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = handle(app, stream).await;
                });
            }
        }
    }
    let _ = app.emit("server-state", ServerState::off(port));
}

/// What the UI shows about the listener: running, off, or refused to bind.
#[derive(Clone, serde::Serialize)]
pub struct ServerState {
    pub running: bool,
    pub port: u16,
    pub error: String,
}

impl ServerState {
    fn up(port: u16) -> Self {
        Self { running: true, port, error: String::new() }
    }
    fn off(port: u16) -> Self {
        Self { running: false, port, error: String::new() }
    }
    fn down(port: u16, error: &str) -> Self {
        Self { running: false, port, error: error.to_string() }
    }
}

// ---- the smallest HTTP/1.1 server that is still correct --------------------

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Read one request: head up to the blank line, then exactly Content-Length
/// bytes of body. No chunked support on purpose — the only client is a `fetch`
/// with a JSON string, which always sends a length.
async fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buf = Vec::with_capacity(2048);
    let mut tmp = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = find_head_end(&buf) {
            break i;
        }
        if buf.len() > MAX_BODY {
            return None;
        }
        let n = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut tmp))
            .await
            .ok()?
            .ok()?;
        if n == 0 {
            return None; // client hung up mid-head
        }
        buf.extend_from_slice(&tmp[..n]);
    };

    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut start = lines.next()?.split_whitespace();
    let method = start.next()?.to_string();
    let path = start.next()?.to_string();

    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();

    let len: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    if len > MAX_BODY {
        return None;
    }

    let mut body = buf.split_off(head_end + 4);
    while body.len() < len {
        let n = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut tmp))
            .await
            .ok()?
            .ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(len);

    Some(Request {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

/// Index of the `\r\n\r\n` that ends the request head.
fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

async fn respond(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        _ => "Error",
    };
    // `Access-Control-Allow-Origin: *` is safe here only because every route
    // that does anything demands the token: a random page can reach the socket
    // whatever we answer with, so the token is the boundary, not the origin.
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: application/json; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: content-type, x-turbograb-token\r\n\
         Access-Control-Max-Age: 86400\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.flush().await
}

fn json_err(code: &str) -> String {
    format!("{{\"ok\":false,\"error\":\"{code}\"}}")
}

async fn handle(app: AppHandle, mut stream: TcpStream) -> std::io::Result<()> {
    let Some(req) = read_request(&mut stream).await else {
        return respond(&mut stream, 400, &json_err("bad_request")).await;
    };
    let path = req.path.split('?').next().unwrap_or("/").to_string();

    // Preflight: the extension sends JSON with a custom header, so every /add
    // is preceded by one of these.
    if req.method == "OPTIONS" {
        return respond(&mut stream, 204, "").await;
    }

    match (req.method.as_str(), path.as_str()) {
        ("GET", "/ping") => {
            let body = format!(
                "{{\"ok\":true,\"app\":\"turbograb\",\"version\":\"{}\",\"paired\":{}}}",
                env!("CARGO_PKG_VERSION"),
                authorized(&app, &req)
            );
            respond(&mut stream, 200, &body).await
        }
        ("POST", "/pair") => {
            let client: PairRequest = serde_json::from_str(&req.body).unwrap_or(PairRequest {
                client: String::new(),
            });
            let origin = req.header("origin").unwrap_or("").to_string();
            match ask_to_pair(&app, &client.client, &origin).await {
                PairOutcome::Paired(token) => {
                    respond(&mut stream, 200, &format!("{{\"ok\":true,\"token\":\"{token}\"}}")).await
                }
                PairOutcome::Denied => respond(&mut stream, 403, &json_err("denied")).await,
                // 429, not 403: the caller may well be legitimate and simply
                // second in line, and the extension retries on its own.
                PairOutcome::Busy => respond(&mut stream, 429, &json_err("pair_in_progress")).await,
            }
        }
        ("POST", "/add") => {
            if !authorized(&app, &req) {
                return respond(&mut stream, 401, &json_err("unauthorized")).await;
            }
            let Ok(add) = serde_json::from_str::<AddRequest>(&req.body) else {
                return respond(&mut stream, 400, &json_err("bad_json")).await;
            };
            if !add.url.starts_with("http://") && !add.url.starts_with("https://") {
                return respond(&mut stream, 400, &json_err("bad_url")).await;
            }
            match crate::add_from_browser(&app, add.url, add.filename, add.headers, add.size) {
                Ok(id) => respond(&mut stream, 200, &format!("{{\"ok\":true,\"id\":\"{id}\"}}")).await,
                Err(e) => respond(&mut stream, 400, &json_err(e.trim_start_matches('@'))).await,
            }
        }
        ("GET", "/downloads") => {
            if !authorized(&app, &req) {
                return respond(&mut stream, 401, &json_err("unauthorized")).await;
            }
            let body = serde_json::to_string(&crate::browser_summary(&app))
                .unwrap_or_else(|_| "[]".into());
            respond(&mut stream, 200, &body).await
        }
        _ => respond(&mut stream, 404, &json_err("not_found")).await,
    }
}

/// Constant-time-ish token comparison. The token is 32 hex characters from the
/// OS RNG; the point of comparing lengths first is only to avoid leaking it
/// through an early return on the first differing byte.
fn token_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The token travels in a header and nowhere else.
///
/// A `?token=` query form used to be accepted here for clients that cannot set
/// a header. It is gone: a URL is the single most copied string in the system,
/// and it ends up in `Referer`, in proxy and access logs, in shell history and
/// in whatever the user pastes into a bug report. A header is copied by nobody.
/// Every client that matters — the extension, curl, a shell one-liner — can set
/// one, so the convenience was never worth the number of places it leaked to.
fn authorized(app: &AppHandle, req: &Request) -> bool {
    let sent = req.header("x-turbograb-token").unwrap_or_default();
    let stored = app.state::<AppState>().settings.lock().unwrap().token.clone();
    token_eq(sent, &stored)
}

/// Ask the user, in the app window, whether this client may pair.
///
/// The prompt is the whole security model of the API, so it blocks: the HTTP
/// request is held open until someone answers or the prompt times out. The
/// window is raised, because a dialog nobody can see cannot be consented to.
/// Why a pairing attempt did not produce a token.
enum PairOutcome {
    Paired(String),
    /// The user said no, or the prompt timed out.
    Denied,
    /// A prompt was already on screen. Never reaches the user.
    Busy,
}

async fn ask_to_pair(app: &AppHandle, client: &str, origin: &str) -> PairOutcome {
    let req_id = crate::random_hex(8);
    let (tx, rx) = oneshot::channel::<bool>();
    {
        // Claim the one slot and register the waiter under a single lock: two
        // requests arriving together must not both find it free.
        let state = app.state::<AppState>();
        let mut pending = state.pending_pairs.lock().unwrap();
        if pending.len() >= MAX_PENDING_PAIRS {
            return PairOutcome::Busy;
        }
        pending.insert(req_id.clone(), tx);
    }
    crate::show_main(app);
    let _ = app.emit(
        "pair-request",
        serde_json::json!({
            "id": req_id,
            "client": client,
            "origin": origin,
            "ts": now_secs(),
        }),
    );

    let allowed = match tokio::time::timeout(PAIR_TIMEOUT, rx).await {
        Ok(Ok(v)) => v,
        // Timed out, or the window went away with the prompt still open.
        _ => false,
    };
    {
        let state = app.state::<AppState>();
        state.pending_pairs.lock().unwrap().remove(&req_id);
        if !allowed {
            let _ = app.emit("pair-closed", req_id);
            return PairOutcome::Denied;
        }
        let mut s = state.settings.lock().unwrap();
        if s.token.is_empty() {
            s.token = crate::random_hex(16);
        }
        let token = s.token.clone();
        drop(s);
        crate::save_settings(app);
        state.paired.store(true, Ordering::Relaxed);
        PairOutcome::Paired(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_only_matches_itself() {
        assert!(token_eq("abc123", "abc123"));
        assert!(!token_eq("abc123", "abc124"));
        assert!(!token_eq("abc123", "abc1234"));
        // An app with no token yet must not be unlocked by sending no token.
        assert!(!token_eq("", ""));
    }

    /// The token must be read from the header and from nowhere else — a URL is
    /// copied into logs and referers, a header is not.
    #[test]
    fn a_token_in_the_query_string_does_not_authorize() {
        let req = Request {
            method: "POST".into(),
            path: "/add?token=deadbeefdeadbeefdeadbeefdeadbeef".into(),
            headers: vec![],
            body: String::new(),
        };
        assert_eq!(req.header("x-turbograb-token"), None);
    }

    #[test]
    fn only_one_pairing_prompt_may_be_open() {
        assert_eq!(
            MAX_PENDING_PAIRS, 1,
            "stacked prompts are how a user gets clicked into handing out a token"
        );
    }

    #[test]
    fn the_head_ends_at_the_blank_line() {
        assert_eq!(find_head_end(b"GET / HTTP/1.1\r\n\r\nbody"), Some(14));
        assert_eq!(find_head_end(b"GET / HTTP/1.1\r\n"), None);
    }
}
