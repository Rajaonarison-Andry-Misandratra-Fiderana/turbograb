#!/usr/bin/env bash
# Sign the Firefox extension for self-hosted distribution.
#
# Mozilla will not install an unsigned add-on in a release build, but it will
# sign one it never lists: the "unlisted" channel uploads the package, signs it,
# and hands the .xpi straight back instead of publishing it on addons.mozilla.org.
# That signed .xpi is what installs in Firefox and its forks.
#
# Credentials are the AMO API key pair, read from a file kept outside the repo so
# the secret is never staged:
#
#   ~/.config/turbograb/amo.env
#     WEB_EXT_API_KEY=user:12345678:123
#     WEB_EXT_API_SECRET=<64 hex chars>
#
# Get the pair at https://addons.mozilla.org/en-US/developers/addon/api/key/
#
# Usage:
#   scripts/sign-extension.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CREDS="${TURBOGRAB_AMO_ENV:-$HOME/.config/turbograb/amo.env}"

if [ -z "${WEB_EXT_API_KEY:-}" ] || [ -z "${WEB_EXT_API_SECRET:-}" ]; then
    if [ ! -f "$CREDS" ]; then
        echo "No AMO credentials: $CREDS is missing." >&2
        echo "Create it with WEB_EXT_API_KEY and WEB_EXT_API_SECRET from" >&2
        echo "https://addons.mozilla.org/en-US/developers/addon/api/key/" >&2
        exit 1
    fi
    # shellcheck disable=SC1090
    set -a; . "$CREDS"; set +a
fi

# Sign what the build produced, so the signed .xpi and the folder loaded
# unpacked during development are the same bytes.
"$ROOT/scripts/build-extension.sh" --dir >/dev/null
npx web-ext lint --source-dir "$ROOT/dist-extension/firefox" --self-hosted

npx web-ext sign \
    --source-dir "$ROOT/dist-extension/firefox" \
    --artifacts-dir "$ROOT/dist-extension/signed" \
    --channel unlisted

echo
echo "Signed:"
ls -1 "$ROOT/dist-extension/signed"/*.xpi
