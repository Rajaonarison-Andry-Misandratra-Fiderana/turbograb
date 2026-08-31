// The toolbar popup: is it connected, is interception on, and what is moving
// right now. Progress is read from the app rather than tracked here — the app
// is the one that knows.

const api = TG.api;
const statusEl = document.getElementById("status");
const listEl = document.getElementById("list");
const emptyEl = document.getElementById("empty");
const enabledEl = document.getElementById("enabled");

function fmtBytes(n) {
  if (!n || n <= 0) return "";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  return `${v.toFixed(i === 0 ? 0 : v < 10 ? 2 : v < 100 ? 1 : 0)} ${units[i]}`;
}

function render(items) {
  const live = items.filter((d) => d.status === "downloading" || d.status === "queued");
  listEl.innerHTML = "";
  emptyEl.hidden = live.length > 0;
  for (const d of live.slice(0, 8)) {
    const li = document.createElement("li");
    li.className = "item";

    const name = document.createElement("div");
    name.className = "name";
    name.textContent = d.title || "…";

    const bar = document.createElement("div");
    bar.className = "bar";
    const fill = document.createElement("i");
    fill.style.width = `${Math.max(0, Math.min(100, d.percent))}%`;
    bar.appendChild(fill);

    const meta = document.createElement("div");
    meta.className = "meta";
    const left = document.createElement("span");
    left.textContent =
      d.status === "queued"
        ? "queued"
        : `${fmtBytes(d.downloaded_bytes)}${d.total_bytes ? ` / ${fmtBytes(d.total_bytes)}` : ""}`;
    const right = document.createElement("span");
    right.textContent = d.speed_bps > 0 ? `${fmtBytes(d.speed_bps)}/s` : "";
    meta.append(left, right);

    li.append(name, bar, meta);
    listEl.appendChild(li);
  }
}

async function refresh() {
  const s = await TG.loadSettings();
  enabledEl.checked = s.enabled;

  const health = await TG.ping(s);
  const ready = health.running && !!s.token;
  statusEl.textContent = !health.running ? "App not running" : !s.token ? "Not paired" : "Connected";
  statusEl.className = `pill ${ready ? "ok" : "bad"}`;

  if (!ready) {
    render([]);
    emptyEl.hidden = false;
    emptyEl.textContent = health.running
      ? "Not paired — open Options and click Connect."
      : "Start TurboGrab to see downloads here.";
    return;
  }
  emptyEl.textContent = "Nothing downloading.";
  const res = await TG.callApp(s, "/downloads");
  render(Array.isArray(res.json) ? res.json : []);
}

enabledEl.addEventListener("change", async () => {
  await TG.saveSettings({ enabled: enabledEl.checked });
  api.runtime.sendMessage({ type: "settings-changed" }).catch(() => {});
});

document.getElementById("options").addEventListener("click", () => {
  api.runtime.openOptionsPage();
  window.close();
});

refresh();
// While the popup is open, keep it live: it is the one place progress is
// visible without switching windows.
const timer = setInterval(refresh, 1000);
window.addEventListener("unload", () => clearInterval(timer));
