<div align="center">

<img src="docs/assets/icon.png" width="104" alt="TurboGrab" />

# TurboGrab

**Click download in your browser. Get the file, faster.**

A Linux desktop download manager. It takes a direct link — pasted in, or caught
from Firefox and Chrome — and fetches it over as many connections as the server
allows. Pause works. Resume actually resumes.

<img alt="Platform" src="https://img.shields.io/badge/platform-Linux-0b7285?style=flat-square&logo=linux&logoColor=white" />
<img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square&logo=tauri&logoColor=white" />
<img alt="Rust" src="https://img.shields.io/badge/Rust-core-CE422B?style=flat-square&logo=rust&logoColor=white" />
<img alt="React 19" src="https://img.shields.io/badge/React-19-61DAFB?style=flat-square&logo=react&logoColor=black" />
<img alt="License" src="https://img.shields.io/badge/license-MIT-555?style=flat-square" />

<br />

<table>
<tr>
<td><img src="docs/assets/screenshot-dark.png" alt="TurboGrab, dark theme" /></td>
<td><img src="docs/assets/screenshot-light.png" alt="TurboGrab, light theme" /></td>
</tr>
</table>

</div>

---

## The idea

Browsers download files on one connection and give up the moment anything goes
wrong. A download manager opens several connections to the same file, asks each
one for a different byte range, and writes them into the same output. On a
server that supports ranges this is several times faster, and it makes a real
pause possible, because you know exactly which bytes you already have.

That is all TurboGrab does. It is not a media extractor: it downloads what a
link actually serves, and it does not parse pages or resolve stream manifests.

## What you get

Paste a link — or a whole block of them — and press **Download**. Install the
browser extension and you skip even that: clicking a download link in Firefox
or Chrome sends it here instead, cookies and all, so files behind a login keep
working.

**Speed.** Up to 16 connections, 4 MiB chunks, work-stealing. When one chunk
crawls, the other connections take the next ones instead of waiting for it. The
strip under the progress bar shows each connection separately, so you can see
where the transfer is actually spending its time.

**Pause and resume that hold.** Byte counts for every chunk are written to a
sidecar file next to the download, so picking up again lands on the exact byte
you stopped at — after a pause, a restart, or a crash. When a server can't
resume, the card tells you before you pause, not after.

**A way out when a link expires.** Signed URLs from S3 or a CDN often die while
a download sits paused. Paste a fresh address and TurboGrab continues onto the
bytes already on disk instead of starting over.

**No surprise overwrites.** A second `video.mp4` becomes `video (2).mp4`. A
`.part` file is only renamed into place once its length matches what the server
promised.

**Numbers, not spinners.** `525 MB / 1.24 GB`, rate, time left, connection
count, and how many retries a flaky link is costing you.

**Somewhere to look when it breaks.** Cards show one plain sentence. The raw
diagnostic is in **Settings → Log**, and on the card's ⋮ menu as *Copy error
details*.

It runs in the tray, can start on boot, and follows your system's light or dark
theme. Interface in French and English, units and dates included.

<a name="install"></a>

## Install

Linux only, installed under `~/.local`. No root, no package manager. You get a
`turbograb` command and an entry in your app launcher.

```bash
git clone https://github.com/Rajaonarison-Andry-Misandratra-Fiderana/turbograb
cd turbograb
npm install
./scripts/install.sh --build
```

If you already have a release binary, drop the `--build`. To remove it, run
`./scripts/uninstall.sh`; your downloads and settings are left alone.

```
~/.local/lib/turbograb/turbograb            the binary
~/.local/bin/turbograb                      wrapper on PATH
~/.local/share/applications/                .desktop entry
~/.local/share/icons/hicolor/*/apps/        icons
```

### The browser extension

```bash
./scripts/build-extension.sh    # unpacked folders, plus .xpi and .zip
```

**Firefox** (and Zen, LibreWolf, Waterfox): open
`about:debugging#/runtime/this-firefox`, choose *Load Temporary Add-on…*, and
pick `dist-extension/firefox/manifest.json`.

That install disappears when the browser restarts, because Firefox release
refuses unsigned add-ons permanently no matter what `xpinstall.signatures.required`
says. To make it stick, either sign it (`npm run ext:lint`, then
`npm run ext:sign` with AMO API keys — see
[extension/README.md](extension/README.md#signing-for-firefox)) or use a build
that honours the pref: Developer Edition, Nightly, LibreWolf.

**Chrome, Chromium, Edge, Brave**: `chrome://extensions`, turn on *Developer
mode*, *Load unpacked*, pick `dist-extension/chrome/`. Unpacked extensions
survive restarts here.

Then open the extension's options and click **Connect**. TurboGrab raises its
window and asks whether to allow it. Allow it once and you're paired.

For the full interception rules, see
[extension/README.md](extension/README.md).

### Starting on boot

Under **Settings → Startup**:

- **Launch on boot** writes an XDG autostart entry with `--hidden`, so logging
  in doesn't throw a window at you.
- **Start in the background** does the same for a manual launch: tray icon
  only. The queue and the browser listener run either way.

Closing the window hides to the tray and downloads keep going. Quit properly
from the tray menu.

## Privacy and security

TurboGrab sends nothing anywhere except the files you ask it to fetch. No
telemetry, no analytics, no update pings, no crash reporting. The browser
extension declares this to Mozilla as `data_collection_permissions: ["none"]`,
and you can check it in a few minutes: there is no bundler, so the extension
sources in this repo are the sources that ship.

The parts worth understanding:

**The local listener.** The extension talks to the app over HTTP on
`127.0.0.1:8787`. Nothing off your machine can reach it, but every program *on*
your machine can — which is why queueing a download requires a token. The token
is 32 hex characters of OS entropy, and the only way to get one is the pairing
prompt in the app window. Revoke it whenever you like in **Settings → Browser
extension**. The token travels in a header, never in a URL, so it doesn't end
up in logs or referers.

**Cookies.** For a file behind a login, the extension sends that URL's cookies
along with the referring page and the browser's user-agent, because without
them the server just returns 403. They go to `127.0.0.1` and nowhere else, they
are stored owner-only (`0600`), and they are **deleted the moment the download
finishes** — a completed transfer never makes another request, so keeping them
would be all risk and no use.

**On disk.** Both files below are written `0600` inside a `0700` directory.

```
~/.local/share/com.fiderana.turbograb/downloads.json   history, and the headers
                                                       of anything still running
~/.local/share/com.fiderana.turbograb/settings.json    folder, language, theme,
                                                       connections, queue depth,
                                                       startup, listener + token
```

The activity log is kept in memory only and never touches the disk.

Found something wrong? Open an issue. If it looks sensitive, say so and leave
the details out of the public thread.

## Settings worth knowing

| Setting | Default | What it changes |
| --- | --- | --- |
| Connections per download | 8 | 1–16. Drop it to 1 for a server that throttles or blocks parallel requests. |
| Simultaneous downloads | 3 | The rest wait as *queued* rather than all sharing the line. |
| Browser listener port | 8787 | Must match the extension's. Loopback only. |
| Minimum size (extension) | 1 MB | Below this, files stay in the browser — the handover costs more than it saves. |

## Development

```bash
npm install
npm run tauri dev
```

```
src/                        React UI — components, MUI theme, i18n, formatting
src-tauri/src/lib.rs        state, settings, queue, commands, tray, startup
src-tauri/src/download.rs   the transfer engine: probe, segments, resume
src-tauri/src/server.rs     the loopback API the extension talks to
extension/                  Firefox + Chrome, one source, two manifests
docs/architecture.md        how it all fits together
```

```bash
cargo test --manifest-path src-tauri/Cargo.toml   # filenames, ranges, resume, queue, tokens
node --test extension/test/                       # the interception rules
npm run ext:lint                                  # the Firefox build, as AMO sees it
npm run build                                     # tsc with noUnusedLocals, then vite
```

Read [docs/architecture.md](docs/architecture.md) before you touch the
downloader, the local API or the theme. It covers the invariants that stop a
segmented download from quietly corrupting its own output, why the API has a
pairing prompt at all, and two MUI traps that produce a light-themed dialog on
a dark window.

<a name="packaging"></a>

## Packaging

| Artifact | Needs anything installed? |
| --- | --- |
| `.AppImage` | No — webkit2gtk is bundled |
| `.deb` | Yes — `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libayatana-appindicator3-1` |

```bash
APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1 npm run tauri build -- --bundles deb,appimage
```

A dev build sits at the same path as a release build and looks identical, but
starts with "could not connect to localhost" because it wants the Vite dev
server. Before shipping, check the frontend is really embedded:

```bash
strings -a src-tauri/target/release/turbograb | grep -c 'assets/index-'   # 1+ is good
```

<details>
<summary><b>Regenerating the icons</b></summary>

`src-tauri/icons/source/icon.svg` is the master. To rebuild everything (all
sizes, plus `.icns`, `.ico`, Android and iOS):

```bash
rsvg-convert -w 1024 -h 1024 src-tauri/icons/source/icon.svg -o /tmp/icon.png
npm run tauri icon -- /tmp/icon.png
```

</details>

## Troubleshooting

**The window is blank white.** On some hybrid Intel + NVIDIA machines,
WebKitGTK's DMABUF renderer aborts inside Mesa's GBM teardown, which kills the
web process and leaves an empty webview. TurboGrab already sets
`WEBKIT_DISABLE_DMABUF_RENDERER=1`. If it still happens, go further:

```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 turbograb
```

**The extension says "App not running".** Check the port matches on both sides
(**Settings → Browser extension** here, *Port* there) and that the listener is
on. If the port was already taken, the reason is in **Settings → Log**.

**Downloads still go through the browser.** They're under the size floor, the
host or file type is excluded, or nothing is paired yet — the toolbar icon
shows `!` in that last case. Right-click → *Download with TurboGrab* ignores
every rule, so it's the quickest way to test the connection.

**The app vanished when I closed it.** It hid to the tray and is still
downloading. Left-click the tray icon to bring it back.

## License

MIT. See [LICENSE](LICENSE).

Download only what you're allowed to.
