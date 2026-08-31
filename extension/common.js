// Shared between the background worker, the popup and the options page.
//
// Loaded as a classic script in all three (Firefox lists it in
// `background.scripts`, Chrome's service worker `importScripts()`es it), so
// everything here hangs off one global rather than being exported.

// Chrome exposes `chrome`, Firefox both `browser` and `chrome`. The callback
// API is identical for what we use, and promises work in both when called
// through `browser` — so normalise once and use promises everywhere.
const api =
  typeof browser !== "undefined" ? browser : typeof chrome !== "undefined" ? chrome : null;

const DEFAULTS = {
  /** Master switch. Off = the browser downloads normally, as if we weren't here. */
  enabled: true,
  port: 8787,
  /** Minted by TurboGrab when you pair. Without it every /add is refused. */
  token: "",
  /** Files smaller than this stay in the browser: for a 40 KB PDF the handover
   *  costs more than the download saves. */
  minSizeMB: 1,
  /** Sizes are often unknown at creation time. Take those anyway? */
  takeUnknownSize: true,
  /** Hosts TurboGrab never touches — one per line, suffix match. Intranets and
   *  anything that hands out a file behind a one-shot session belong here. */
  skipHosts: "",
  /** Extensions TurboGrab never touches, comma separated, no dots. */
  skipExts: "",
  /** Show a notification for each handover. */
  notify: true,
};

/** Everything the extension remembers, defaults filled in. */
async function loadSettings() {
  const stored = await api.storage.local.get(DEFAULTS);
  return { ...DEFAULTS, ...stored };
}

async function saveSettings(patch) {
  await api.storage.local.set(patch);
}

const baseUrl = (port) => `http://127.0.0.1:${port}`;

/** One request to the app. Never throws: every caller wants a verdict, not an
 *  exception, because "TurboGrab isn't running" is a normal state. */
async function callApp(settings, path, { method = "GET", body, timeoutMs = 4000 } = {}) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const res = await fetch(baseUrl(settings.port) + path, {
      method,
      signal: controller.signal,
      headers: {
        "Content-Type": "application/json",
        ...(settings.token ? { "X-TurboGrab-Token": settings.token } : {}),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await res.text();
    let json = null;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      /* a non-JSON body is a failure whatever it says */
    }
    return { ok: res.ok, status: res.status, json };
  } catch {
    return { ok: false, status: 0, json: null };
  } finally {
    clearTimeout(timer);
  }
}

/** Is the app up, and does it accept our token? */
async function ping(settings) {
  const res = await callApp(settings, "/ping", { timeoutMs: 1500 });
  return {
    running: res.ok,
    paired: !!res.json?.paired,
    version: res.json?.version ?? "",
  };
}

/** Ask the app for a token. Resolves once the user answers the prompt in the
 *  app window — which is why the timeout is long. */
async function pair(settings, clientName) {
  const res = await callApp(settings, "/pair", {
    method: "POST",
    body: { client: clientName },
    timeoutMs: 125000,
  });
  if (res.ok && res.json?.token) return { ok: true, token: res.json.token };
  return { ok: false, error: res.status === 0 ? "unreachable" : res.json?.error || "denied" };
}

/** Host of a URL, or "" for anything we can't parse (blob:, data:, …). */
function hostOf(url) {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}

function extOf(name) {
  const clean = (name || "").split(/[?#]/)[0];
  const dot = clean.lastIndexOf(".");
  return dot > 0 ? clean.slice(dot + 1).toLowerCase() : "";
}

/** Basename of a path, both separators — Chrome hands back OS-shaped paths. */
function baseName(path) {
  const parts = (path || "").split(/[\\/]/);
  return parts[parts.length - 1] || "";
}

function parseList(text) {
  return (text || "")
    .split(/[\s,;]+/)
    .map((s) => s.trim().toLowerCase().replace(/^\./, ""))
    .filter(Boolean);
}

/** Should this download be handed to TurboGrab?
 *
 *  Pure, and exported on its own, because this is the decision users complain
 *  about when it is wrong — so it is the part that has tests.
 *  Returns a reason string when the answer is no. */
function decide(item, settings) {
  if (!settings.enabled) return "disabled";
  // Only a URL the app can fetch again on its own. A blob: or data: body lives
  // in the page, and filesystem downloads have nothing to accelerate.
  if (!/^https?:\/\//i.test(item.url || "")) return "not-http";
  const host = hostOf(item.url);
  if (!host) return "no-host";
  // Our own loopback API, and anything served from this machine: handing those
  // to a multi-connection downloader helps nobody.
  if (host === "127.0.0.1" || host === "localhost" || host === "[::1]") return "local";
  if (parseList(settings.skipHosts).some((h) => host === h || host.endsWith("." + h)))
    return "skip-host";

  const name = baseName(item.filename) || item.url;
  const ext = extOf(name);
  if (ext && parseList(settings.skipExts).includes(ext)) return "skip-ext";

  const size = Number(item.fileSize ?? item.totalBytes ?? 0);
  if (size > 0) {
    if (size < settings.minSizeMB * 1000 * 1000) return "too-small";
  } else if (!settings.takeUnknownSize) {
    return "unknown-size";
  }
  return "";
}

// The options page and the tests reach these through the global; the service
// worker gets them by importScripts, which shares this same scope.
const TG = {
  api,
  DEFAULTS,
  loadSettings,
  saveSettings,
  callApp,
  ping,
  pair,
  decide,
  hostOf,
  extOf,
  baseName,
  parseList,
};

// Node (the test runner) has no `self`; browsers have no `module`.
if (typeof module !== "undefined") module.exports = TG;
