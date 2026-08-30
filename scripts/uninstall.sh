#!/usr/bin/env bash
#
# Remove everything scripts/install.sh put under ~/.local.
# User data (downloads, settings) is untouched.

set -euo pipefail

APP_ID="turbograb"
LIBDIR="$HOME/.local/lib/$APP_ID"
BINDIR="$HOME/.local/bin"
DESKTOP_DIR="$HOME/.local/share/applications"
ICON_ROOT="$HOME/.local/share/icons/hicolor"

rm -rf "$LIBDIR"
rm -f "$BINDIR/$APP_ID"
rm -f "$DESKTOP_DIR/$APP_ID.desktop"
for size in 32x32 128x128 256x256 512x512; do
    rm -f "$ICON_ROOT/$size/apps/$APP_ID.png"
done

command -v update-desktop-database >/dev/null && update-desktop-database "$DESKTOP_DIR" || true
command -v gtk-update-icon-cache  >/dev/null && gtk-update-icon-cache -qtf "$ICON_ROOT" 2>/dev/null || true

echo "TurboGrab uninstalled."
