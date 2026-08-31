// The options page. Writes straight through to storage on every change — an
// extension settings page with a Save button is a settings page people leave
// half-applied.

const api = TG.api;

const F = {
  enabled: document.getElementById("enabled"),
  port: document.getElementById("port"),
  token: document.getElementById("token"),
  minSizeMB: document.getElementById("minSizeMB"),
  takeUnknownSize: document.getElementById("takeUnknownSize"),
  skipHosts: document.getElementById("skipHosts"),
  skipExts: document.getElementById("skipExts"),
  notify: document.getElementById("notify"),
};
const statusEl = document.getElementById("status");
const savedEl = document.getElementById("saved");
const connectBtn = document.getElementById("connect");
const forgetBtn = document.getElementById("forget");
const pairHint = document.getElementById("pairHint");

let savedTimer = 0;
function flashSaved() {
  savedEl.hidden = false;
  clearTimeout(savedTimer);
  savedTimer = setTimeout(() => (savedEl.hidden = true), 1200);
}

async function fill() {
  const s = await TG.loadSettings();
  F.enabled.checked = s.enabled;
  F.port.value = s.port;
  F.token.value = s.token;
  F.minSizeMB.value = s.minSizeMB;
  F.takeUnknownSize.checked = s.takeUnknownSize;
  F.skipHosts.value = s.skipHosts;
  F.skipExts.value = s.skipExts;
  F.notify.checked = s.notify;
  forgetBtn.disabled = !s.token;
  await refreshStatus();
}

async function patch(next) {
  await TG.saveSettings(next);
  flashSaved();
  api.runtime.sendMessage({ type: "settings-changed" }).catch(() => {});
  await refreshStatus();
}

/** One line that says whether this extension can actually do anything. */
async function refreshStatus() {
  const s = await TG.loadSettings();
  const health = await TG.ping(s);
  const ready = health.running && !!s.token;
  statusEl.textContent = !health.running
    ? "App not running"
    : !s.token
      ? "Not paired"
      : `Connected · v${health.version}`;
  statusEl.className = `pill ${ready ? "ok" : "bad"}`;
  forgetBtn.disabled = !s.token;
  pairHint.textContent = !health.running
    ? `Nothing answering on 127.0.0.1:${s.port}. Start TurboGrab, then click Connect.`
    : s.token
      ? "Paired. Downloads over the size threshold are handed to the app."
      : "Click Connect, then allow the request in the TurboGrab window.";
}

F.enabled.addEventListener("change", () => patch({ enabled: F.enabled.checked }));
F.notify.addEventListener("change", () => patch({ notify: F.notify.checked }));
F.takeUnknownSize.addEventListener("change", () =>
  patch({ takeUnknownSize: F.takeUnknownSize.checked }),
);
F.port.addEventListener("change", () => {
  const n = Number(F.port.value);
  // A bad port would silently stop every handover; refuse it rather than store it.
  if (Number.isInteger(n) && n >= 1024 && n <= 65535) patch({ port: n });
  else TG.loadSettings().then((s) => (F.port.value = s.port));
});
F.minSizeMB.addEventListener("change", () => {
  const n = Number(F.minSizeMB.value);
  patch({ minSizeMB: Number.isFinite(n) && n >= 0 ? n : TG.DEFAULTS.minSizeMB });
});
F.skipHosts.addEventListener("change", () => patch({ skipHosts: F.skipHosts.value }));
F.skipExts.addEventListener("change", () => patch({ skipExts: F.skipExts.value }));
F.token.addEventListener("change", () => patch({ token: F.token.value.trim() }));

connectBtn.addEventListener("click", async () => {
  const s = await TG.loadSettings();
  connectBtn.disabled = true;
  connectBtn.textContent = "Waiting for approval…";
  pairHint.textContent = "TurboGrab is asking you to allow this extension. Answer in its window.";

  const name = navigator.userAgent.includes("Firefox") ? "Firefox extension" : "Chrome extension";
  const res = await TG.pair(s, name);

  connectBtn.disabled = false;
  connectBtn.textContent = "Connect";
  if (res.ok) {
    F.token.value = res.token;
    await patch({ token: res.token });
  } else {
    pairHint.textContent =
      res.error === "unreachable"
        ? `Nothing answering on 127.0.0.1:${s.port}. Is TurboGrab running?`
        : "The request was refused in TurboGrab.";
    await refreshStatus();
  }
});

forgetBtn.addEventListener("click", async () => {
  F.token.value = "";
  await patch({ token: "" });
});

fill();
