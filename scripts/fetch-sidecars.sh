#!/usr/bin/env bash
# Fetch the bundled sidecars into src-tauri/binaries/ under the names Tauri's
# `externalBin` expects (<stem>-<target-triple>[.exe]).
#
# WHY THIS SCRIPT EXISTS
# ----------------------
# The ffmpeg/ffprobe sidecars were once copied straight from this machine's
# /usr/bin. Those are dynamically linked against libavcodec.so.NN & friends, so
# they stopped running the moment the distro bumped ffmpeg, and they never ran at
# all on a user's machine. The failure was silent: ffmpeg_location() quietly fell
# back to a system ffmpeg on the dev box, so everything looked fine here and
# broke everywhere else.
#
# So: never copy a distro ffmpeg in here. Only fully static builds, and verify
# with `ldd` that nothing links against libav*.
#
# Usage:  scripts/fetch-sidecars.sh [linux|windows|all]     (default: all)
#         FORCE=1 scripts/fetch-sidecars.sh                 re-download everything
#
# Existing files are kept by default — these are 80 MB each and a re-run after a
# single failed download should not drag the other five over the wire again.
# Use FORCE=1 to refresh, which is worth doing periodically for yt-dlp.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/src-tauri/binaries"
WHAT="${1:-all}"

# Static ffmpeg/ffprobe, one self-contained binary each, no shared libav.
FFMPEG_TAG="b6.1.1"
FFMPEG_BASE="https://github.com/eugeneware/ffmpeg-static/releases/download/$FFMPEG_TAG"
# yt-dlp ships its own Python runtime, so "latest" is safe and wants to be recent.
YTDLP_BASE="https://github.com/yt-dlp/yt-dlp/releases/latest/download"

LINUX_TRIPLE="x86_64-unknown-linux-gnu"
WIN_TRIPLE="x86_64-pc-windows-msvc"

mkdir -p "$OUT"

get() {  # <url> <dest>
    if [ -s "$2" ] && [ "${FORCE:-0}" != 1 ]; then
        echo "  ok $(basename "$2") (present; FORCE=1 to refresh)"
        return
    fi
    echo "  -> $(basename "$2")"
    curl -fsSL --retry 8 --retry-all-errors --retry-delay 4 --max-time 1800 -o "$2.tmp" "$1"
    mv "$2.tmp" "$2"
    chmod +x "$2"
}

if [ "$WHAT" = all ] || [ "$WHAT" = linux ]; then
    echo "==> Linux ($LINUX_TRIPLE)"
    get "$FFMPEG_BASE/ffmpeg-linux-x64"  "$OUT/ffmpeg-$LINUX_TRIPLE"
    get "$FFMPEG_BASE/ffprobe-linux-x64" "$OUT/ffprobe-$LINUX_TRIPLE"
    get "$YTDLP_BASE/yt-dlp_linux"       "$OUT/yt-dlp-$LINUX_TRIPLE"
fi

if [ "$WHAT" = all ] || [ "$WHAT" = windows ]; then
    echo "==> Windows ($WIN_TRIPLE)"
    get "$FFMPEG_BASE/ffmpeg-win32-x64"  "$OUT/ffmpeg-$WIN_TRIPLE.exe"
    get "$FFMPEG_BASE/ffprobe-win32-x64" "$OUT/ffprobe-$WIN_TRIPLE.exe"
    get "$YTDLP_BASE/yt-dlp.exe"         "$OUT/yt-dlp-$WIN_TRIPLE.exe"
fi

echo
echo "==> Verifying the Linux sidecars are self-contained"
fail=0
for f in "$OUT/ffmpeg-$LINUX_TRIPLE" "$OUT/ffprobe-$LINUX_TRIPLE"; do
    [ -f "$f" ] || continue
    if ldd "$f" 2>&1 | grep -q 'libav'; then
        echo "  FAIL $(basename "$f") links against shared libav — not portable"
        fail=1
    elif ! "$f" -version >/dev/null 2>&1; then
        echo "  FAIL $(basename "$f") does not run"
        fail=1
    else
        echo "  ok   $(basename "$f")"
    fi
done

echo
echo "==> sha256"
( cd "$OUT" && sha256sum ./* 2>/dev/null || true )

exit $fail
