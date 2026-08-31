#!/usr/bin/env bash
# Package the browser extension for Chrome and Firefox.
#
# The two stores want the same files under a different manifest, so each target
# is assembled in its own staging directory with `manifest.<target>.json` copied
# in as `manifest.json`. Nothing is generated: what ships is what is in
# extension/, which is also what you load unpacked.
#
# Both forms are produced side by side: the unpacked folder is what you load
# during development, the archive is what you upload (and, for Firefox, what
# `about:addons -> Install Add-on From File` takes — hence the .xpi name).
#
# Usage:
#   scripts/build-extension.sh            # unpacked folders + archives
#   scripts/build-extension.sh --dir      # unpacked folders only
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$ROOT/extension"
OUT="$ROOT/dist-extension"
ZIP=1
[ "${1:-}" = "--dir" ] && ZIP=0

# Everything the extension needs at runtime. Listed rather than globbed so a
# stray note or an editor backup never ends up in a store upload.
FILES=(common.js background.js options.html options.js popup.html popup.js ui.css icons)

rm -rf "$OUT"
mkdir -p "$OUT"

for target in chrome firefox; do
    stage="$OUT/$target"
    mkdir -p "$stage"
    for f in "${FILES[@]}"; do
        cp -r "$SRC/$f" "$stage/"
    done
    cp "$SRC/manifest.$target.json" "$stage/manifest.json"

    echo "  -> dist-extension/$target/  (load unpacked)"

    if [ "$ZIP" = 1 ]; then
        # Firefox installs an .xpi; a Chrome store upload wants a .zip. Same
        # bytes either way — the extension decides what the name has to be.
        ext=zip
        [ "$target" = firefox ] && ext=xpi
        (cd "$stage" && zip -qr "$OUT/turbograb-$target.$ext" .)
        echo "  -> dist-extension/turbograb-$target.$ext"
    fi
done

echo
echo "Firefox: about:debugging#/runtime/this-firefox -> Load Temporary Add-on"
echo "         -> dist-extension/firefox/manifest.json"
echo "Chrome:  chrome://extensions -> Developer mode -> Load unpacked"
echo "         -> dist-extension/chrome/"
