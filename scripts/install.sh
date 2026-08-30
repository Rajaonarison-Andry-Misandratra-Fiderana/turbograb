#!/usr/bin/env bash
#
# Install TurboGrab as a native Linux app under ~/.local (no root, no dpkg).
#
#   ~/.local/lib/turbograb/     ytfetcher + the three sidecars
#   ~/.local/bin/turbograb      wrapper on PATH -> `turbograb` from any shell
#   ~/.local/share/...          .desktop entry + hicolor icons -> app launcher
#
# The sidecars MUST sit next to the binary: lib.rs resolves `.sidecar("yt-dlp")`
# verbatim against <exe_dir>. Hence the private lib dir rather than dropping four
# executables straight into ~/.local/bin, where `ffmpeg`/`ffprobe`/`yt-dlp` would
# shadow (or be shadowed by) whatever the user already has on PATH.
#
# Usage: ./scripts/install.sh [--build]
#   --build   run `npm run tauri build -- --no-bundle` first

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_ID="turbograb"
APP_NAME="TurboGrab"
BIN_NAME="ytfetcher"        # cargo package name; also the X11 WM class
TRIPLE="x86_64-unknown-linux-gnu"

LIBDIR="$HOME/.local/lib/$APP_ID"
BINDIR="$HOME/.local/bin"
DESKTOP_DIR="$HOME/.local/share/applications"
ICON_ROOT="$HOME/.local/share/icons/hicolor"

RELEASE="$ROOT/src-tauri/target/release"
SRC_BIN="$RELEASE/$BIN_NAME"

if [ "${1:-}" = "--build" ]; then
    echo "==> Building release binary (frontend + rust)"
    ( cd "$ROOT" && npm run tauri build -- --no-bundle )
fi

if [ ! -x "$SRC_BIN" ]; then
    echo "error: $SRC_BIN not found." >&2
    echo "       Run: ./scripts/install.sh --build" >&2
    exit 1
fi

echo "==> Installing binaries to $LIBDIR"
mkdir -p "$LIBDIR"
install -m 755 "$SRC_BIN" "$LIBDIR/$BIN_NAME"

# Sidecars come from src-tauri/binaries/, where they carry the target-triple
# suffix. Tauri strips that suffix when bundling; we do the same by hand so this
# works whether or not the last build ran the bundler.
for tool in yt-dlp ffmpeg ffprobe; do
    src="$ROOT/src-tauri/binaries/$tool-$TRIPLE"
    if [ ! -f "$src" ]; then
        echo "error: missing sidecar $src" >&2
        exit 1
    fi
    install -m 755 "$src" "$LIBDIR/$tool"
done

echo "==> Installing launcher to $BINDIR/$APP_ID"
mkdir -p "$BINDIR"
cat > "$BINDIR/$APP_ID" <<EOF
#!/bin/sh
# TurboGrab launcher — exec so signals and the exit code pass straight through.
#
# This used to force the GTK dark theme, because the window wore a GTK titlebar
# and the UI underneath was dark-only. Neither holds now: the window is
# borderless and the UI follows the system light/dark scheme. The only GTK
# surface left is the folder picker, and forcing dark there would make it
# disagree with the app on a light desktop. So: inherit the session's theme.

# WebKitGTK's DMABUF renderer aborts inside Mesa's GBM teardown on some hybrid
# GPUs, which blanks the window. The app sets this itself; exporting it here too
# means it also applies if the binary is run through a different wrapper.
: "\${WEBKIT_DISABLE_DMABUF_RENDERER:=1}"
export WEBKIT_DISABLE_DMABUF_RENDERER

exec "$LIBDIR/$BIN_NAME" "\$@"
EOF
chmod 755 "$BINDIR/$APP_ID"

echo "==> Installing icons to $ICON_ROOT"
# 128x128@2x.png is a 256x256 image; icon.png is 512x512.
install_icon() {  # <size-dir> <source file>
    mkdir -p "$ICON_ROOT/$1/apps"
    install -m 644 "$ROOT/src-tauri/icons/$2" "$ICON_ROOT/$1/apps/$APP_ID.png"
}
install_icon 32x32   "32x32.png"
install_icon 128x128 "128x128.png"
install_icon 256x256 "128x128@2x.png"
install_icon 512x512 "icon.png"

echo "==> Installing desktop entry to $DESKTOP_DIR"
mkdir -p "$DESKTOP_DIR"
# StartupWMClass must be the *binary* name, not the app id: Tauri derives the
# window's WM class from the executable, so the launcher only matches the
# running window if these agree.
cat > "$DESKTOP_DIR/$APP_ID.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$APP_NAME
Comment=Download YouTube video and audio, or any direct file link
Comment[fr]=Télécharge des vidéos et de l'audio YouTube, ou tout lien de fichier direct
Exec=$BINDIR/$APP_ID %u
Icon=$APP_ID
Terminal=false
Categories=AudioVideo;Video;
Keywords=youtube;download;downloader;video;audio;file;telecharger;
StartupWMClass=$BIN_NAME
EOF
chmod 644 "$DESKTOP_DIR/$APP_ID.desktop"

command -v update-desktop-database >/dev/null && update-desktop-database "$DESKTOP_DIR" || true
command -v gtk-update-icon-cache  >/dev/null && gtk-update-icon-cache -qtf "$ICON_ROOT" 2>/dev/null || true

echo
echo "Installed. Launch from the app menu, or run: $APP_ID"

case ":$PATH:" in
    *":$BINDIR:"*) ;;
    *)
        echo
        echo "warning: $BINDIR is not on your PATH. For fish, run once:"
        echo "    fish_add_path $BINDIR"
        ;;
esac
