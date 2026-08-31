<div align="center">

<img src="docs/assets/icon.png" width="104" alt="TurboGrab" />

# TurboGrab

**Click download in your browser. Get the file, faster.**

A desktop download manager that takes any direct link — pasted in, or
intercepted from Firefox and Chrome — and fetches it as fast as the server
allows, across up to 16 connections, with a pause that actually resumes.

<a href="#install"><img alt="Platform" src="https://img.shields.io/badge/platform-Linux-0b7285?style=flat-square&logo=linux&logoColor=white" /></a>
<img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square&logo=tauri&logoColor=white" />
<img alt="Rust" src="https://img.shields.io/badge/Rust-core-CE422B?style=flat-square&logo=rust&logoColor=white" />
<img alt="React 19" src="https://img.shields.io/badge/React-19-61DAFB?style=flat-square&logo=react&logoColor=black" />
<img alt="Extension" src="https://img.shields.io/badge/extension-Firefox%20%2B%20Chrome-ff7139?style=flat-square&logo=firefoxbrowser&logoColor=white" />

<br />

<table>
<tr>
<td><img src="docs/assets/screenshot-dark.png" alt="TurboGrab, dark theme" /></td>
<td><img src="docs/assets/screenshot-light.png" alt="TurboGrab, light theme" /></td>
</tr>
</table>

</div>

---

## What it does

Paste one link or a whole block of them, press **Download**, and TurboGrab
splits each file across parallel connections wherever the server supports byte
ranges. Install the browser extension and you don't even do that: clicking a
download link in Firefox or Chrome sends it here, cookies included.

## Why you might want it

**It is genuinely fast.** Range-capable servers are fetched in 4 MiB chunks over
up to 16 connections with a work-stealing queue, so a single slow chunk never
leaves the other connections idle. The strip under the progress bar shows the
real shape of the transfer, connection by connection.

**Pause actually resumes.** Per-chunk byte counts live in a sidecar file next to
the download, so resuming picks up exactly where it stopped — after a pause,
after a restart, after a crash. If a server can't resume, the card says so
*before* you pause.

**Expired links are recoverable.** Signed URLs from S3, CloudFront or a CDN die
while a download is paused. TurboGrab keeps every byte already on disk and asks
for a fresh address instead of starting over — the equivalent of IDM's "refresh
download address".

**It takes your browser's downloads.** The extension hands the URL over with the
cookies, referer and user-agent the browser was about to use, which is what
makes a file behind a login work outside the browser. If the app isn't running,
the download is handed straight back and the browser keeps it — you can't lose a
file by having TurboGrab closed.

**It never overwrites.** A second copy of `video.mp4` becomes `video (2).mp4`,
and a `.part` is only renamed into place once its length matches the size the
server promised.

**It tells you the numbers.** `525 MB / 1.24 GB`, transfer rate, time left,
connection count, and how many retries a flaky connection is costing you.

**It stays out of the way.** Launch on boot, start straight to the tray, keep
downloading with no window open. Errors are one plain sentence, with the raw
diagnostic tucked into **Settings → Log** for when you actually want it.

Light and dark follow your system. French and English throughout, units and
dates included.

<a name="install"></a>

## Install

Linux only. Native install under `~/.local` — no root, no package manager.
You get both a `turbograb` command and an app-launcher entry.

```bash
git clone https://github.com/Rajaonarison-Andry-Misandratra-Fiderana/turbograb
cd turbograb
npm install
./scripts/install.sh --build
```

Already have a release binary? `./scripts/install.sh`.
Changed your mind? `./scripts/uninstall.sh` — your downloads and settings stay.

```
~/.local/lib/turbograb/turbograb            the binary
~/.local/bin/turbograb                      wrapper on PATH
~/.local/share/applications/                .desktop entry
~/.local/share/icons/hicolor/*/apps/        icons
```

### The browser extension

```bash
./scripts/build-extension.sh           # unpacked folders + turbograb-firefox.xpi / -chrome.zip
```

- **Firefox** (and forks — Zen, LibreWolf, Waterfox) —
  `about:debugging#/runtime/this-firefox` → *Load Temporary Add-on…* →
  `dist-extension/firefox/manifest.json`. That install lasts until the browser
  restarts: Firefox release refuses unsigned add-ons permanently. For a lasting
  one, sign it — `npm run ext:lint` then `npm run ext:sign` with AMO API keys,
  see [extension/README.md](extension/README.md#signing-for-firefox) — or use a
  build where `xpinstall.signatures.required=false` takes effect (Developer
  Edition, Nightly, LibreWolf).
- **Chrome / Chromium / Edge / Brave** — `chrome://extensions` → *Developer
  mode* → *Load unpacked* → `dist-extension/chrome/`. Unpacked extensions
  survive restarts here.

Then open the extension's options and click **Connect**: TurboGrab raises its
window and asks whether to allow it. That prompt is the whole security model —
the app listens on `127.0.0.1` only, and the token it hands out is the only way
to queue a download. Revoke it any time in **Settings → Browser extension**.

Full details, including exactly which downloads get intercepted:
[extension/README.md](extension/README.md).

### Launch on boot, and staying in the background

**Settings → Startup**:

- **Launch on boot** writes your platform's own autostart entry (an XDG
  `.desktop` on Linux) with `--hidden`, so logging in doesn't throw a window at
  you.
- **Start in the background** applies the same to a manual launch: no window,
  just the tray icon — the queue and the browser listener are up regardless.

Closing the window has always meant "hide to the tray"; downloads keep running.
Quit for real from the tray menu.

### Where your data lives

```
~/.local/share/com.fiderana.turbograb/downloads.json   download history
~/.local/share/com.fiderana.turbograb/settings.json    folder, language, theme,
                                                       connections, queue depth,
                                                       startup, listener + token
```

## Settings worth knowing

| Setting | Default | What it changes |
| --- | --- | --- |
| Connections per download | 8 | 1–16. Set it to 1 for a server that throttles or blocks parallel requests. |
| Simultaneous downloads | 3 | The rest wait as *queued* instead of sharing the line. |
| Browser listener port | 8787 | Must match the extension's. Loopback only. |
| Minimum size (extension) | 1 MB | Smaller files stay in the browser, where the handover would cost more than it saves. |

## Development

```bash
npm install
npm run tauri dev
```

```
src/                     React UI — components, MUI theme, i18n, formatting
src-tauri/src/lib.rs     state, settings, the queue, commands, tray, startup
src-tauri/src/download.rs   the transfer engine: probe, segments, resume
src-tauri/src/server.rs     the loopback API the extension talks to
extension/               Firefox + Chrome extension, one source, two manifests
docs/architecture.md     How it all fits together
```

Tests:

```bash
cargo test --manifest-path src-tauri/Cargo.toml   # filenames, ranges, resume, queue, tokens
node --test extension/test/                       # the interception rules
npm run ext:lint                                  # the Firefox build, as AMO sees it
npm run build                                     # tsc with noUnusedLocals, then vite
```

**Read [docs/architecture.md](docs/architecture.md)** before changing the
downloader, the local API or the theme — it documents the invariants that keep a
segmented download from silently corrupting its output, the reason the API has a
pairing prompt at all, and two MUI traps that quietly produce a light-themed
dialog on a dark window.

<a name="packaging"></a>

## Packaging

| Artifact | Needs anything installed? |
| --- | --- |
| `.AppImage` | **No** — bundles webkit2gtk |
| `.deb` | Yes — declares `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libayatana-appindicator3-1` |

```bash
APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1 npm run tauri build -- --bundles deb,appimage
```

Before shipping `src-tauri/target/release/turbograb`, check it is a real release
build. A dev build can sit at that same path and look identical, but starts with
"could not connect to localhost" because it wants the Vite dev server:

```bash
strings -a src-tauri/target/release/turbograb | grep -c 'assets/index-'   # 1+ = frontend embedded
```

<details>
<summary><b>Regenerating the icons</b></summary>

`src-tauri/icons/source/icon.svg` is the master. To rebuild the whole set (all
sizes plus `.icns` / `.ico` / Android / iOS):

```bash
rsvg-convert -w 1024 -h 1024 src-tauri/icons/source/icon.svg -o /tmp/icon.png
npm run tauri icon -- /tmp/icon.png
```

</details>

## Troubleshooting

**The window is blank white.** WebKitGTK's DMABUF renderer aborts inside Mesa's
GBM teardown on some hybrid Intel + NVIDIA machines, killing the web process and
leaving an empty webview. TurboGrab sets `WEBKIT_DISABLE_DMABUF_RENDERER=1`
itself; if it still happens, go one step further:

```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 turbograb
```

**The extension says "App not running".** Check the port matches on both sides
(**Settings → Browser extension** here, *Port* there), and that the listener is
enabled. If the port was taken, the reason is in **Settings → Log**.

**Downloads still go through the browser.** They are under the extension's size
floor, the host or file type is excluded, or nothing is paired yet — the toolbar
icon shows `!` in that case. Right-click → *Download with TurboGrab* ignores
every rule and is the quickest way to check the connection.

**A download failed and the message is vague.** That is deliberate — cards show
one short sentence. The exact text is in **Settings → Log**, and on the card's
⋮ menu as *Copy error details*.

**The app vanished when I closed it.** It hid to the system tray and is still
downloading. Left-click the tray icon to bring it back, or use the tray menu to
quit for real.

## Notes

- TurboGrab downloads what a link actually serves. It does not parse pages,
  resolve stream manifests, or bundle a media extractor.
- Only download what you are allowed to. The extension reads cookies to keep a
  download working outside the browser; they go to `127.0.0.1` and nowhere else.
