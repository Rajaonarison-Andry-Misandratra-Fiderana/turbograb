# TurboGrab — architecture

How the pieces fit, and the handful of rules that are not obvious from the code.
Read the sections marked **⚠ Invariant** before touching the downloader; read
**Theming** before touching the UI.

```
┌──────── browser ────────┐          ┌──────────── webview (React 19 + MUI 9) ────────────┐
│ extension/              │          │  src/App.tsx        shell, settings, dialogs       │
│  background.js  decide  │          │  src/components/*   composer, cards, settings, log │
│  → cancel + POST /add   │          │  src/theme.ts       Material 3 tokens, light+dark  │
└───────────┬─────────────┘          └───────────▲──────────────────────────┬─────────────┘
            │ http, 127.0.0.1 only               │ download-update          │ invoke(...)
            │ token in a header                  │ pair-request             ▼
┌───────────▼────────────────────────────────────┴──────────────────────────────────────┐
│ Rust core                                                                              │
│   lib.rs        AppState { items, settings, log, pairs }, queue, commands, tray        │
│   server.rs     the loopback API: /ping /pair /add /downloads                          │
│   download.rs   probe → segmented or single, N connections, 4 MiB chunks, .part/.meta  │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

The Rust side owns everything durable. The webview holds no state that matters:
close it, reopen it, and `list_downloads` restores the world. That is also why
the app keeps working with no window at all — see **Startup and background**.

## IPC surface

All commands are registered in `run()`.

| Command | Purpose |
| --- | --- |
| `add_downloads(urls, outDir)` | Queue one or more links. Returns the ids created. |
| `pause_download(id)` | Signal the transfer task. Keeps the `.part`, frees a slot. |
| `resume_download(id)` | Start again from where the bytes stop, or queue it. |
| `resume_all()` | Every `interrupted` + `paused` item, oldest first. |
| `refresh_link(id, url)` | Swap a dead signed URL for a live one, keeping the `.part`. |
| `cancel_download(id, deleteFile)` | Remove from the list, optionally delete from disk. |
| `clear_finished(deleteFiles)` | Same, for every `done` + `error` item. |
| `list_downloads()` | Full snapshot. The UI's only bootstrap. |
| `open_file(id)` · `reveal_file(id)` | Open in the default app / select in the file manager. |
| `files_present(ids)` | Which finished files are still on disk. |
| `get_settings()` · `set_settings(settings)` | Everything the app remembers. |
| `get_log()` · `clear_log()` | The diagnostic journal. |
| `pair_respond(id, allow)` | Answer a pairing prompt from the extension. |
| `get_token()` · `ensure_token()` · `revoke_token()` | The local API's credential. |
| `server_state()` | Whether the listener is on, its port, and whether anything is paired. |
| `launched_hidden()` | Should this launch stay in the tray? |

Events flow the other way:

| Event | Payload | When |
| --- | --- | --- |
| `download-update` | a whole `DownloadInfo` | any change to any item |
| `download-removed` | a bare id | cancel |
| `pair-request` | `{ id, client, origin, ts }` | an extension asked to pair |
| `pair-closed` | request id | the prompt timed out |
| `paired` | the token, or `""` | pairing succeeded, or was revoked |
| `server-state` | `{ running, port, error }` | the listener came up or refused to bind |

### Why open/reveal are Rust commands

`tauri-plugin-opener` exposes `open_path` to the webview, but only under a path
scope — granting it would mean writing `{"path": "**"}` into the capability
file. Resolving the path in Rust instead means the opener only ever sees a path
the app itself produced, and lets us return a typed `@missing_file` when the
user has moved the file behind our back. `files_present` exists so the card can
hide *Open* rather than offer an action that fails.

## State and persistence

| File | Contents | Written |
| --- | --- | --- |
| `<app_data>/downloads.json` | `Vec<DownloadInfo>` | after every command, and on every 2 % of progress |
| `<app_data>/settings.json` | folder, language, theme, connections, queue depth, startup, listener, token | on change, via write-then-rename |
| `<file>.part` | the bytes so far | continuously |
| `<file>.part.meta` | `SegMeta { total, validator, done[] }` | at each throttled progress tick |

**Every field added to `DownloadInfo` after the first release carries
`#[serde(default)]`.** A `downloads.json` written by an older build must still
load, or a user loses their whole history on upgrade. Fields *removed* are just
as safe: serde ignores what it doesn't know, which is what lets a file written
before this version still load today.

Not persisted: `segments` (a 48-entry array rewritten several times a second has
no business in an on-disk history — `save()` clears it on the way out) and the
in-memory `log`, which covers the running session only. `error_detail` *is*
persisted: it is small, and it is the only thing that explains a failure after a
restart.

On load, `downloading` and `queued` both become `interrupted`: neither survives
a process exit, and the resume banner is how the user gets them back.

## The transfer engine

`probe()` establishes size, resumability, the redirect target and a validator,
then either `run_segmented` (N connections, 4 MiB chunks, work-stealing) or
`run_single` (one connection, resumed from the `.part` length).

**⚠ Invariant — a `206` is the only safe answer to a `Range` request.** A `200`
carries the whole file; writing it at a chunk's offset splices a full copy into
that chunk's slot. `probe()` trusts an observed `206`, never the
`Accept-Ranges` header, which servers advertise and then ignore.

**⚠ Invariant — same size is not the same file.** The `ETag`/`Last-Modified`
a download started against is persisted in the `.meta` sidecar and replayed as
`If-Range`; a mismatch throws the bytes away instead of resuming onto them.

**⚠ Invariant — a short body is not a finished chunk.** A stream that ends
cleanly but under its requested range is retried, not marked complete.

**⚠ Invariant — a short `.part` is never renamed into place.** Every path that
finishes a transfer goes through `commit()`, which compares the file's length
against the expected total first. A truncated file that *looks* finished is the
one failure a downloader must never produce.

Break any of the four and the output file is corrupted *while still reporting
success*. All four have tests.

Tuning constants, all in `download.rs`: `DEFAULT_CONNECTIONS = 8` (settable
1–16), `CHUNK = 4 MiB`, `MIN_SEGMENTED = 4 MiB` (below this, one connection is
plenty), `WRITE_BUF = 256 KiB`, `MAX_RETRY = 5` with exponential backoff.

### Naming

A name is resolved once, on the first run: `Content-Disposition` (RFC 5987 form
preferred), else the last segment of the *final* URL after redirects, else the
original URL. It is then sanitized — path separators and the characters NTFS
rejects become `_`, trailing dots go — and de-duplicated by `unique_name`,
which treats both an existing file and an in-flight `.part` as taken. Two
downloads of the same URL produce `file.zip` and `file (2).zip` instead of one
silently overwriting the other.

The result is persisted before the first byte lands, so a resume reuses the same
name rather than stepping to the next free one and stranding what is on disk.

### The queue

`max_active` downloads run at once; the rest sit in `queued`. Every entry point
— the composer, a resume, `resume_all`, the extension — goes through
`start_or_queue`, and every exit — done, failed, paused, cancelled, limit raised
— calls `pump`, which fills free slots oldest-first. Nothing else may spawn a
transfer directly.

### Progress throttling

Segmented downloads emit at most ~4×/second, gated by a lock-free CAS on a
timestamp — whichever worker wins the compare-exchange emits, so there is no
mutex in the per-chunk hot path.

`segments` is bucketed to **48 slots** regardless of file size. A 4 GB file is
1024 chunks; sending those raw would be ~10 KB of JSON four times a second, and
no progress bar can show more ticks than it has pixels.

## The browser integration

### The listener

`server.rs` is a hand-written HTTP/1.1 server on `127.0.0.1:<port>` (default
8787). Four routes, tiny JSON bodies — not worth a framework's dependency tree.

| Route | Auth | Purpose |
| --- | --- | --- |
| `GET /ping` | none | "running", plus version and whether your token is good |
| `POST /pair` | the user | asks in the app window; returns a token if allowed |
| `POST /add` | token | `{ url, filename, headers, size }` → a queued download |
| `GET /downloads` | token | compact progress list for the extension popup |

**⚠ The socket is loopback-only, but loopback is not a boundary.** Every program
and every page on this machine can reach the port. So `/add` and `/downloads`
require a token minted from OS entropy (`getrandom`), and the *only* way to get
one is `/pair`, which parks the HTTP connection while the app raises its window
and asks the user, naming the client and its origin. A timeout denies. `/ping`
is unauthenticated on purpose and answers nothing but "running".

`Access-Control-Allow-Origin: *` is set because a page can reach the socket
whatever we answer with — the token is the boundary, not the origin header.

### The extension

`extension/` builds for both Chrome (MV3 service worker) and Firefox (MV3 event
page) from one source, with two manifests. The handover:

1. `downloads.onCreated` fires; `decide()` applies the rules (size floor, host
   and file-type exclusions, `http(s)` only, never loopback).
2. If a ping in the last 15 seconds says the app is reachable **and** the
   extension holds a token, the browser's download is cancelled and erased.
3. The URL is POSTed with the request headers the browser would have used:
   `Cookie` (read via the `cookies` API), `Referer`, and the browser's own
   `User-Agent`. A session-gated file 403s without them.
4. **If the POST fails, the download is put back** via
   `downloads.download({url})`, with the URL held in a bypass set so the
   re-created download isn't intercepted into a loop.

Nothing is cancelled on a stale verdict, and nothing is lost when the app is
closed. That safety net is the reason the rules can be permissive by default.

On the app's side, the headers travel with the item and are persisted, because a
resume tomorrow needs them as much as the first attempt did. `usable_headers`
drops the ones that describe *that* request rather than this one — `Range`,
`If-Range`, `Accept-Encoding`, `Host`, `Content-Length`, hop-by-hop headers —
because replaying a `Range` would fight the segmenter and a `gzip`
`Accept-Encoding` would make the body length disagree with the size we
pre-allocate.

## Startup and background

`tauri-plugin-autostart` writes the platform's own autostart entry (XDG
`.desktop`, LaunchAgent, `Run` key) with `--hidden` appended, and
`apply_autostart` pushes the setting to it whenever it changes.

A launch is hidden when `--hidden` is present *or* `start_hidden` is set. The
frontend asks `launched_hidden()` before revealing the window, and the Rust
fallback timer that shows the window after three seconds is skipped in that
case. Everything else still runs: the queue, the tray, and the browser listener.

`tauri-plugin-single-instance` is registered **first**, so a second launch hands
its arguments to the running instance and exits rather than failing to bind the
API port half-way through startup. Its callback also raises the window and
queues any URLs on the command line — `turbograb https://…` works from a shell.

## Errors

Nothing raw ever reaches a card. `classify()` reduces every failure to a code:

```
@network  @server  @not_found  @blocked  @rate_limited  @no_space  @no_write
@link_expired  @no_range  @short_read  @bad_link  @busy  @no_dir  @missing_file
```

The UI turns the code into a sentence (`src/errors.ts` → `t.errors[code]`), and
anything unrecognised becomes a generic message rather than leaking a two
hundred character error about a DNS failure.

**⚠ `classify()` is only ever applied where a message is about to be shown**,
never inside the retry loop. `is_fatal()` treats a leading `@` as "do not
retry", and a dropped connection is exactly what deserves retrying.

The original text is not lost. It goes to two places:

- `error_detail` on the item — offered as *Copy error details* on the card's ⋮
  menu, and persisted with the download.
- The **journal**, a 400-entry ring buffer in `AppState`, fed by download
  failures, every retry (`retry 2/5 after: …`), and the listener refusing to
  bind. Read it in **Settings → Log**.

## Frontend

```
src/App.tsx                 shell: settings, list, dialogs, snackbar, pairing
src/types.ts                mirrors DownloadInfo + Settings — keep in step with lib.rs by hand
src/format.ts               bytes, durations, relative times — all locale-aware
src/errors.ts               code → sentence
src/theme.ts                Material 3 palette, type scale, component overrides
src/i18n.ts                 FR/EN; `Dict` is inferred from the French object
src/hooks/useDownloads.ts   snapshot + the two event listeners + actions
src/components/*            one file per surface
```

`useDownloads` registers the `listen()` subscriptions and returns their
unsubscribe functions from the effect. React 19's StrictMode mounts effects
twice; without that cleanup every card would update twice per tick.

The composer takes **any number of links**: `parseLinks` splits on whitespace,
keeps only `http(s)` URLs and de-duplicates, so a copied block of links is one
paste and one click. The count under the field is the confirmation before
anything is queued.

## Theming

Material 3 on MUI 9, one theme carrying both colour schemes, published as CSS
variables (`cssVariables: { colorSchemeSelector: "data" }`).

**⚠ Trap 1 — `theme.palette.*` is frozen to the default scheme.** With
`cssVariables` enabled it returns a literal hex from the *light* scheme, not a
reactive value:

```ts
theme.palette.surfaceContainer.high       // "#E4E9EC"  ← always light
theme.vars.palette.surfaceContainer.high  // "var(--mui-palette-…)"  ← follows the scheme
```

Reading the first inside an `sx` or `styleOverrides` callback paints light
surfaces onto a dark window — a white dialog with white text on it. Component
files therefore import **`tok`** from `src/theme.ts`, a map of raw CSS
variables, and never touch `theme.palette`.

**⚠ Trap 2 — a bare number in `sx.borderRadius` is a multiplier.**
`borderRadius: 3` means `3 × theme.shape.borderRadius` = 36px, not 3px. Always
write `` `${shape.md}px` ``.

The corner scale, applied everywhere, with one rule: **an inner radius is always
smaller than the one it sits inside.**

| | radius |
| --- | --- |
| buttons, chips, toggles, progress bars | pill |
| inputs and small controls | 8 |
| cards, menus, alerts | 12 |
| containers that hold cards or inputs | 16 |
| dialogs | 28 |

**Status is said once.** Each state owns exactly one signal: a determinate bar
for a transfer, an indeterminate bar plus one line for `queued`, an alert for
`error`. No pill, no coloured rail, no recoloured bar saying the same thing
three times.

Type is **Inter**, bundled offline. The Material 3 type scale ships tracking
drawn for Roboto, which reads loose on Inter — every variant is retuned
negative, and `cv05` is enabled so a lowercase `l` grows a tail and stops
reading as a `1` in a filename.

## Window chrome and platform notes

The window runs with `decorations: false` so tiling compositors get the window
itself rather than a server-side frame. That makes the app responsible for the
whole frame:

- `TitleBar` carries `data-tauri-drag-region` (double-click to maximise comes
  free) and a single close button — closing hides to the tray, which the Rust
  side intercepts via `prevent_close()`.
- `ResizeHandles` puts eight invisible grips on the edges, because Wayland draws
  no resize border without CSD.

The window starts `visible: false` and the frontend reveals it after its first
paint; a fixed `backgroundColor` can only ever be right for one colour scheme.
A Rust timer shows it anyway after three seconds — unless the launch was meant
to be hidden — so a frontend that fails to boot cannot leave an invisible app
running.

**⚠ `WEBKIT_DISABLE_DMABUF_RENDERER=1`** is set at the top of `run()`. On hybrid
Intel + NVIDIA machines WebKitGTK's DMABUF renderer aborts inside Mesa's GBM
teardown — the backtrace runs `exit → libwebkit2gtk → libgbm → dri_gbm →
libgallium → free()` and glibc kills the process with *corrupted double-linked
list*. The web process dies, the UI process survives, and the user sees a blank
white window. It is only set when the variable is unset, so an explicit `0`
still wins.

## Things deliberately not built

- **A global speed limit.** Connections per download and queue depth already
  cover "stop saturating my line", without a token bucket in the hot path.
- **OS notifications from the app.** Completion raises an in-app snackbar; the
  extension is the one that notifies, because it already has the permission.
- **Native messaging instead of HTTP.** It would remove the token and the port,
  but needs a per-browser manifest installed into a per-OS path by a privileged
  installer — a heavier install for a boundary the pairing prompt already draws.
- **Media-site extraction.** TurboGrab downloads what a link actually serves. It
  does not parse pages, resolve stream manifests, or bundle an extractor.
