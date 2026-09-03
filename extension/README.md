# TurboGrab browser extension

Hands your browser's downloads to TurboGrab: the file is fetched over several
connections, pause and resume actually work, and the transfer survives closing
the tab.

One codebase, two manifests — Chrome (MV3 service worker) and Firefox (MV3
event page). Everything else in this folder is shared.

```
common.js      settings, the API client, and decide() — the interception rules
background.js  the handover: cancel, send, restore on failure
popup.*        toolbar popup: connection state + what is downloading
options.*      settings page: pairing, port, rules
```

## Install (unpacked, for development)

```bash
scripts/build-extension.sh     # -> dist-extension/{chrome,firefox}/ + .zip / .xpi
```

Each target gets its own manifest copied in as `manifest.json`, so load the
*built* folder, never this source folder.

**Firefox**, and the forks that follow it (Zen, LibreWolf, Waterfox) —
`about:debugging#/runtime/this-firefox` → *Load Temporary Add-on…* →
`dist-extension/firefox/manifest.json`.

That lasts until the browser restarts. Firefox release will not install an
unsigned add-on permanently, whatever `xpinstall.signatures.required` says — so
either [sign it](#signing-for-firefox), or use a build that honours the pref:
Developer Edition, Nightly, or LibreWolf.

**Chrome / Chromium / Edge / Brave** — `chrome://extensions` → enable
*Developer mode* → *Load unpacked* → `dist-extension/chrome/`. This one sticks
across restarts.

## Signing for Firefox

Mozilla signs the add-on; you keep distributing it yourself. Two channels:

| Channel | Review | Where it ends up |
| --- | --- | --- |
| `unlisted` | automated, usually a couple of minutes | a signed `.xpi` you host or install by hand |
| `listed` | human review, days | published on addons.mozilla.org |

Unlisted is what you want for a companion extension to a desktop app.

**Once, to get API credentials.** Sign in at addons.mozilla.org, then
[Developer Hub → Manage API Keys](https://addons.mozilla.org/developers/addon/api/key/).
It shows a **JWT issuer** (`user:12345:67`) and a **JWT secret**. The secret is
displayed once — store it in a password manager, never in this repo.

**Every release:**

```bash
npm run ext:lint                       # 0 errors, 0 warnings before you upload

# bump the version in BOTH manifests first — AMO refuses a version it has
# already signed, and the two manifests must not drift apart.

WEB_EXT_API_KEY='user:12345:67' WEB_EXT_API_SECRET='…' npm run ext:sign
```

The signed file lands in `dist-extension/signed/`. Install it from
`about:addons` → the gear → **Install Add-on From File**, and it survives
restarts.

Requirements the signing service actually enforces, all already satisfied here:

- **A stable extension id** — `browser_specific_settings.gecko.id`
  (`turbograb@fiderana`). It is the identity AMO signs against; change it and
  you have a different add-on.
- **A declared data-collection stance** —
  `gecko.data_collection_permissions.required: ["none"]`. Honest: the extension
  reads cookies for the download it is handing over and sends them to
  `127.0.0.1`. Nothing is collected by, or transmitted to, the developer.
  Declaring the key requires `strict_min_version` ≥ 142, which is why it is set
  there.
- **No build step to disclose.** Sources are shipped as written — no bundler, no
  minifier — so no source-code upload is required.

### Chrome

Chrome has no equivalent of unlisted signing. Either keep loading the unpacked
folder (it survives restarts) or publish through the Chrome Web Store, which
needs a developer account with a one-off fee. A self-packed `.crx` is refused by
stock Chrome on install, so it is not a route.

## Pairing

1. Start TurboGrab. It listens on `127.0.0.1:8787` (Settings → Browser
   extension).
2. Open the extension's options, click **Connect**.
3. TurboGrab raises its window and asks whether to allow the extension. Allow
   it once; the token is stored by the extension from then on.

Nothing on the network can reach the app — the listener is bound to loopback —
but every program on *this machine* can, which is why the token exists and why
only that prompt hands one out. Revoke it any time in TurboGrab's settings, or
in **Forget** here.

## What gets intercepted

A download is handed over when all of these hold (`decide()` in `common.js`,
tested in `test/decide.test.mjs`):

| Rule | Default |
| --- | --- |
| Interception is on | yes |
| The URL is `http(s)` | `blob:`, `data:` and `file:` always stay in the browser |
| The host isn't loopback | `127.0.0.1`, `localhost` stay in the browser |
| The host isn't in your exclusion list | empty |
| The file type isn't in your exclusion list | empty |
| It is at least *minimum size* | 1 MB |
| …or its size is unknown and you allow those | allowed |

The right-click menu (**Download with TurboGrab** on a link, image, video or
audio) ignores every rule above: it means *this one*, now.

### The safety net

The browser's download is only cancelled when a ping in the last 15 seconds
says the app is reachable, and if the POST still fails the download is handed
straight back to the browser. Closing TurboGrab mid-browse costs you nothing.

## Cookies, referer, user-agent

They travel with the download. A session-gated file 403s the moment its URL
leaves the browser, so the extension reads the cookies for that URL and sends
them along with the referring page and the browser's own user-agent; TurboGrab
replays those headers on every connection.

They are also the most sensitive thing this extension touches, so: they go to
`127.0.0.1` and nowhere else, they are stored by the app owner-only (`0600`),
and TurboGrab **deletes them the moment the download finishes** — a completed
transfer never makes another request, so there is nothing left to keep them
for.

## Why each permission

The `<all_urls>` one is broad, and it should be the first thing you question.
Here is the whole list and what breaks without each.

| Permission | Why |
| --- | --- |
| `<all_urls>` | The narrower forms don't work here. You can download a file from *any* host, and the extension has to read that host's cookies to make the transfer work outside the browser — so the set of hosts it needs is "wherever you download from", which is not a list anyone can write in advance. It buys **no** content-script access: this extension injects nothing into any page. |
| `cookies` | Read the cookies for the one URL being handed over. Nothing enumerates cookies for other sites. |
| `downloads` | See the download that is starting, cancel it, and hand it back if the app can't be reached. |
| `storage` | Remember your settings and the pairing token. Local only. |
| `contextMenus` | The **Download with TurboGrab** right-click entry. |
| `notifications` | Say when a download has been handed over. Switch it off in options. |
| `alarms` | Poll the app for progress while the popup is closed. |
| `http://127.0.0.1/*`, `http://localhost/*` | Talk to TurboGrab. The only host the extension ever sends anything to. |

No analytics, no remote config, no update pings, no bundler. The sources you
read here are the sources that ship — which is also why AMO needs no
source-code upload from us.

## Tests

```bash
node --test extension/test/
```
