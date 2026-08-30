# TurboGrab — architecture

How the pieces fit, and the handful of rules that are not obvious from the code.
Read the sections marked **⚠ Invariant** before touching the downloader; read
**Theming** before touching the UI.

```
┌──────────────── webview (React 19 + MUI 9) ────────────────┐
│  src/App.tsx          shell, settings, dialogs             │
│  src/components/*     composer, cards, settings, log       │
│  src/theme.ts         Material 3 tokens, light + dark      │
└───────────▲───────────────────────────────┬────────────────┘
            │ download-update               │ invoke(...)
            │ download-removed              ▼
┌───────────┴──────────────── Rust core (src-tauri/src/lib.rs) ─┐
│  AppState { items, settings, log }                            │
│      │                        │                               │
│      │ yt-dlp path            │ direct path                   │
│      ▼                        ▼                               │
│  sidecar child process    reqwest, 8 connections, 4 MiB chunks │
└───────────────────────────────────────────────────────────────┘
```

The Rust side owns everything durable. The webview holds no state that matters:
close it, reopen it, and `list_downloads` restores the world.

## IPC surface

Eighteen commands, all registered in `run()`.

| Command | Purpose |
| --- | --- |
| `fetch_info(id, url, kind)` | Phase 1 for media: `yt-dlp -J` dumps metadata and formats without downloading. |
| `retry_fetch(id)` | Re-run phase 1 for an item that failed to analyse. |
| `start_download(id, quality, outDir)` | Phase 2: spawn `yt-dlp` for the chosen format. |
| `start_file_download(id, url, outDir)` | Direct HTTP download; no metadata phase. |
| `pause_download(id)` | Kill the child / signal the streaming task. Keeps the `.part`. |
| `resume_download(id)` | Respawn from where the bytes stop. |
| `resume_all()` | Every `interrupted` + `paused` item. Returns the ids it touched. |
| `refresh_link(id, url)` | Swap a dead signed URL for a live one, keeping the `.part`. |
| `cancel_download(id, deleteFile)` | Remove from the list, optionally delete from disk. |
| `clear_finished(deleteFiles)` | Same, for every `done` + `error` item. |
| `list_downloads()` | Full snapshot. The UI's only bootstrap. |
| `open_file(id)` · `reveal_file(id)` | Open in the default app / select in the file manager. |
| `files_present(ids)` | Which finished files are still on disk. |
| `get_settings()` · `set_settings(settings)` | Folder, language, theme, tray hint. |
| `get_log()` · `clear_log()` | The diagnostic journal. |

Two events flow the other way: **`download-update`** carries a whole
`DownloadInfo`, **`download-removed`** carries a bare id.

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
| `<app_data>/settings.json` | folder, language, theme, tray hint | on change, via write-then-rename |
| `<file>.part` | the bytes so far | continuously |
| `<file>.part.meta` | `SegMeta { total, validator, done[] }` | at each throttled progress tick |

**Every field added to `DownloadInfo` after the first release carries
`#[serde(default)]`.** A `downloads.json` written by an older build must still
load, or a user loses their whole history on upgrade.

Two fields are deliberately *not* persisted:

- `segments` — a 48-entry array rewritten several times a second has no business
  in an on-disk history. `save()` clears it on the way out.
- `error_detail` is persisted (it is small and useful after a restart), but the
  in-memory `log` is not: it covers the running session only.

On load, `downloading` becomes `interrupted` and `fetching` becomes an error
with `@fetch_interrupted`, because neither can survive a process exit.

## The two download engines

### yt-dlp (media)

`build_args` assembles the invocation. Three parts of it are load-bearing:

- **`--print after_move:FILE|%(filepath)s` + `--no-simulate`.** This is the only
  reliable source of the final filename. `[download] Destination:` names the
  *pre-merge* stream, so a `bv*+ba` merge or an `-x mp3` extraction leaves it
  pointing at a file that no longer exists — which is why deleting a finished
  download used to silently delete nothing. `--print` implies `--simulate`
  unless `--no-simulate` is present.
- **⚠ `--progress`.** `--print` also implies `--quiet`, which silences the
  progress lines the UI lives on. Verified: without `--progress`, **zero**
  `PROG|` lines reach the app. Do not remove it.
- **`--progress-template`** carries raw `downloaded_bytes`, `total_bytes`,
  `speed` and `eta` alongside the pre-formatted strings, so the UI can render
  them in the user's own units.

Pause kills the child and keeps the `.part`; resume respawns with `-c`.

> A `bv*+ba` download fetches two streams in sequence, so the percentage runs
> 0→100 twice. A `phase` field would fix that; it is not implemented.

### Direct files

`probe()` establishes size, resumability, the redirect target and a validator,
then either `run_segmented` (8 connections, 4 MiB chunks, work-stealing) or
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

Break any of the three and the output file is corrupted *while still reporting
success*.

Tuning constants, all in `lib.rs`: `CONNECTIONS = 8`, `CHUNK = 4 MiB`,
`MIN_SEGMENTED = 4 MiB` (below this, one connection is plenty),
`WRITE_BUF = 256 KiB`, `MAX_RETRY = 5` with exponential backoff.

### Progress throttling

Segmented downloads emit at most ~4×/second, gated by a lock-free CAS on a
timestamp — whichever worker wins the compare-exchange emits, so there is no
mutex in the per-chunk hot path.

`segments` is bucketed to **48 slots** regardless of file size. A 4 GB file is
1024 chunks; sending those raw would be ~10 KB of JSON four times a second, and
no progress bar can show more ticks than it has pixels.

## Errors

Nothing raw ever reaches a card. `classify()` reduces every failure to a code:

```
@network  @server  @not_found  @blocked  @rate_limited  @unsupported
@no_space @no_write @link_expired @no_ffmpeg @no_range @short_read @bad_link …
```

The UI turns the code into a sentence (`src/errors.ts` → `t.errors[code]`), and
anything unrecognised becomes a generic message rather than leaking a two
hundred character Python traceback about a DNS failure.

**⚠ `classify()` is only ever applied where a message is about to be shown**,
never inside the retry loop. `is_fatal()` treats a leading `@` as "do not
retry", and a dropped connection is exactly what deserves retrying.

The original text is not lost. It goes to two places:

- `error_detail` on the item — offered as *Copy error details* on the card's ⋮
  menu, and persisted with the download.
- The **journal**, a 400-entry ring buffer in `AppState`, fed by analysis
  failures, download failures, every retry (`retry 2/5 after: …`) and a missing
  ffmpeg. Read it in **Settings → Log**.

## Frontend

```
src/App.tsx              shell: settings, list, dialogs, snackbar
src/source.ts            which engine a pasted link needs
src/types.ts             mirrors DownloadInfo — keep in step with lib.rs by hand
src/format.ts            bytes, durations, relative times — all locale-aware
src/errors.ts            code → sentence
src/theme.ts             Material 3 palette, type scale, component overrides
src/i18n.ts              FR/EN; `Dict` is inferred from the French object
src/hooks/useDownloads.ts   snapshot + the two event listeners + actions
src/components/*         one file per surface
```

`useDownloads` registers the `listen()` subscriptions and returns their
unsubscribe functions from the effect. React 19's StrictMode mounts effects
twice; without that cleanup every card would update twice per tick.

### Source detection

`detect()` routes YouTube hosts to `yt-dlp` and everything else to the direct
downloader. It is deliberately *not* clever: guessing "some site yt-dlp might
know" would silently route real file links away from the accelerated path. The
picker in the link field exists to correct it, and the correction is dropped as
soon as the link it applied to changes.

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

**Status is said once.** It used to be encoded three times — a pill, a coloured
rail down the card's left edge, and a recoloured progress bar. The badge now
appears only for `downloading`, `paused`, `interrupted` and `done`; `fetching`,
`ready` and `error` each already have their own signal on the row below
(indeterminate bar, quality picker, alert).

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
A Rust timer shows it anyway after three seconds, so a frontend that fails to
boot cannot leave an invisible app running.

**⚠ `WEBKIT_DISABLE_DMABUF_RENDERER=1`** is set at the top of `run()`. On hybrid
Intel + NVIDIA machines WebKitGTK's DMABUF renderer aborts inside Mesa's GBM
teardown — the backtrace runs `exit → libwebkit2gtk → libgbm → dri_gbm →
libgallium → free()` and glibc kills the process with *corrupted double-linked
list*. The web process dies, the UI process survives, and the user sees a blank
white window. It is only set when the variable is unset, so an explicit `0`
still wins.

## Things deliberately not built

- **A queue.** Every download starts immediately, and `resume_all` respawns all
  of them at once. A `max_concurrent` scheduler needs a `queued` status, which
  ripples into the sort order, the status vocabulary, the badge rules and
  `load()`.
- **OS notifications.** Completion raises an in-app snackbar. A real
  notification needs a new plugin, a new capability, a runtime permission
  prompt, and a notification daemon on Linux.
- **Configurable connection count.** `CONNECTIONS` is threaded through the hot
  path; exposing it read-only first is the safer order.
