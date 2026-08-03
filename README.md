# TurboGrab

Download manager desktop app — Tauri 2 + React/TS.

Two sources, one list:

- **YouTube** — video (mp4) or audio (mp3), via bundled `yt-dlp`
- **Fichier** — any direct file URL, streamed over HTTP

Shared features:

- **Pause / resume** per download
- Live progress (percent / speed / ETA) streamed from Rust to the UI
- Pick output directory

## Architecture

```
src/                     React UI (tabs, download list, controls)
src-tauri/src/lib.rs     Rust backend: commands + progress events + state
src-tauri/binaries/      Bundled sidecars (yt-dlp, ffmpeg, ffprobe), per target-triple
```

- **UI → Rust**: `fetch_info`, `start_download` (YouTube), `start_file_download`
  (direct file), `pause_download`, `resume_download`, `cancel_download`,
  `list_downloads` (Tauri commands).
- **Rust → UI**: `download-update` / `download-removed` events.
- **YouTube** download = a `yt-dlp` sidecar child process. Pause kills the child
  (keeps the `.part`); resume respawns with `-c`, continuing the byte range.
- **Direct file** download = an async HTTP GET (reqwest) streamed to `<name>.part`,
  renamed on completion. Pause aborts the stream; resume sends an HTTP `Range`
  request from the current `.part` size, so it continues instead of restarting.

## Dev

```bash
npm install
npm run tauri dev
```

In dev, ffmpeg is found on `PATH`. In a bundled build the sidecars next to the
executable are used automatically.

## Sidecar binaries

`src-tauri/binaries/` must contain, per platform target-triple:

- `yt-dlp-<triple>` — standalone build from the yt-dlp GitHub releases
- `ffmpeg-<triple>`, `ffprobe-<triple>` — needed for merging video+audio and mp3

Current triple: `x86_64-unknown-linux-gnu`. Add matching binaries for other
platforms before cross-building.

## Notes

- Downloading from YouTube may violate its ToS. Personal / educational use.
- Keep `yt-dlp` updated — YouTube changes break older versions.
