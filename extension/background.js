// Intercepts browser downloads and hands them to TurboGrab.
//
// THE HANDOVER, and why it is shaped like this:
//   1. `downloads.onCreated` fires. We decide (see `decide` in common.js).
//   2. If we take it, the browser's own download is cancelled and erased, and
//      the URL is POSTed to the app together with the request headers the
//      browser would have used — cookies above all, without which a
//      session-gated link 403s the moment it leaves the browser.
//   3. If the app can't be reached, the download is put back: the browser
//      re-downloads it itself. Losing a file because a desktop app was closed
//      is not an acceptable failure mode, so nothing is cancelled until a
//      recent ping says the app is there, and a failed POST restores it.
//
// Chrome runs this as a service worker (which can be killed at any moment, so
// no state lives in module scope that isn't rebuilt on demand); Firefox as an
// event page. Both load `common.js` first — Chrome via importScripts below.

try {
  // Firefox lists common.js in `background.scripts`, so it is already here.
  if (typeof TG === "undefined") importScripts("common.js");
} catch (e) {
  /* Firefox: no importScripts, and TG is already defined. */
}

const api = TG.api;

/** URLs we put back after a failed handover. The re-created download must not
 *  be intercepted again, or a closed app turns into an infinite loop. */
const bypass = new Set();

/** Cached reachability, so `onCreated` can decide without awaiting a probe. */
let health = { running: false, paired: false, checkedAt: 0 };

async function refreshHealth() {
  const settings = await TG.loadSettings();
  const res = await TG.ping(settings);
  health = { ...res, checkedAt: Date.now() };
  await paintBadge();
  return health;
}

/** The toolbar icon carries one bit: can this extension do its job right now? */
async function paintBadge() {
  const settings = await TG.loadSettings();
  const action = api.action || api.browserAction;
  if (!action) return;
  const off = !settings.enabled;
  const text = off ? "off" : health.running && settings.token ? "" : "!";
  const color = off ? "#6b7280" : health.running ? "#0b7285" : "#b42318";
  try {
    await action.setBadgeText({ text });
    await action.setBadgeBackgroundColor({ color });
    await action.setTitle({
      title: off
        ? "TurboGrab — interception off"
        : !health.running
          ? "TurboGrab — app not reachable"
          : !settings.token
            ? "TurboGrab — not paired yet"
            : "TurboGrab — ready",
    });
  } catch {
    /* the action API is unavailable while the browser is starting up */
  }
}

function notify(title, message) {
  try {
    api.notifications?.create({
      type: "basic",
      iconUrl: api.runtime.getURL("icons/icon-128.png"),
      title,
      message,
    });
  } catch {
    /* notifications are optional; never let one break a handover */
  }
}

/** The headers the app must replay to fetch this URL outside the browser. */
async function headersFor(url, referrer) {
  const headers = {};
  // The browser's own UA: CDNs fingerprint it, and a request that looks like a
  // different client is exactly the one that gets a 403.
  if (navigator?.userAgent) headers["User-Agent"] = navigator.userAgent;
  if (referrer && /^https?:/i.test(referrer)) headers["Referer"] = referrer;

  try {
    const cookies = await api.cookies.getAll({ url });
    if (cookies?.length) {
      headers["Cookie"] = cookies.map((c) => `${c.name}=${c.value}`).join("; ");
    }
  } catch {
    // No cookie permission, or a container/private window we can't read. The
    // download may still work; a public file needs no session at all.
  }
  return headers;
}

/** Hand one download over. Returns true when the app took it. */
async function sendToApp(settings, { url, filename, referrer, size }) {
  const headers = await headersFor(url, referrer);
  const res = await TG.callApp(settings, "/add", {
    method: "POST",
    timeoutMs: 8000,
    body: { url, filename: TG.baseName(filename), headers, size: Math.max(0, Number(size) || 0) },
  });
  if (res.ok) return true;
  if (res.status === 401) {
    notify("TurboGrab", "Not paired — open the extension options and click Connect.");
  }
  return false;
}

/** Put a download back in the browser's hands, exactly once. */
async function restore(url) {
  bypass.add(url);
  try {
    await api.downloads.download({ url });
  } catch {
    notify("TurboGrab", "Couldn't hand the download over, and couldn't restart it either.");
  }
  // Long enough for the re-created download to be created and seen once.
  setTimeout(() => bypass.delete(url), 60_000);
}

api.downloads.onCreated.addListener(async (item) => {
  if (bypass.has(item.url)) {
    bypass.delete(item.url);
    return;
  }
  const settings = await TG.loadSettings();
  if (TG.decide(item, settings) !== "") return;

  // Never cancel on a stale verdict: if the last ping is old, take one now.
  if (Date.now() - health.checkedAt > 15_000) await refreshHealth();
  if (!health.running || !settings.token) return; // the browser keeps it

  try {
    await api.downloads.cancel(item.id);
    await api.downloads.erase({ id: item.id });
  } catch {
    return; // already finished or gone — leave it alone
  }

  const ok = await sendToApp(settings, {
    url: item.url,
    filename: item.filename,
    referrer: item.referrer,
    size: item.fileSize ?? item.totalBytes ?? 0,
  });
  if (!ok) {
    await restore(item.url);
    return;
  }
  if (settings.notify) {
    notify("Sent to TurboGrab", TG.baseName(item.filename) || TG.hostOf(item.url));
  }
});

// ---- explicit handover: the right-click menu ------------------------------
//
// Independent of the interception rules on purpose. "Download with TurboGrab"
// means *this one*, whatever the size filter or the host exclusions say.

const MENU_ID = "turbograb-download";

function buildMenu() {
  const menus = api.contextMenus || api.menus;
  if (!menus) return;
  menus.removeAll(() => {
    menus.create({
      id: MENU_ID,
      title: "Download with TurboGrab",
      contexts: ["link", "image", "video", "audio"],
    });
  });
}

(api.contextMenus || api.menus)?.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId !== MENU_ID) return;
  const url = info.linkUrl || info.srcUrl;
  if (!url || !/^https?:\/\//i.test(url)) {
    notify("TurboGrab", "That link can't be downloaded outside the browser.");
    return;
  }
  const settings = await TG.loadSettings();
  const ok = await sendToApp(settings, {
    url,
    filename: TG.baseName(new URL(url).pathname),
    referrer: info.pageUrl || tab?.url,
    size: 0,
  });
  notify("TurboGrab", ok ? "Sent to TurboGrab" : "TurboGrab didn't take it — check the options.");
  await refreshHealth();
});

// ---- wiring ---------------------------------------------------------------

api.runtime.onInstalled.addListener(() => {
  buildMenu();
  refreshHealth();
});
api.runtime.onStartup?.addListener(() => {
  buildMenu();
  refreshHealth();
});

// The popup and the options page ask for a fresh verdict rather than keeping
// their own; one place owns the health state.
api.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg?.type === "health") {
    refreshHealth().then((h) => sendResponse(h));
    return true; // async response
  }
  if (msg?.type === "settings-changed") {
    refreshHealth();
  }
  return false;
});

// A periodic ping keeps the badge honest — and, in Chrome, keeps the verdict
// warm enough that `onCreated` rarely has to wait for one.
api.alarms?.create("turbograb-ping", { periodInMinutes: 1 });
api.alarms?.onAlarm.addListener((alarm) => {
  if (alarm.name === "turbograb-ping") refreshHealth();
});

refreshHealth();
