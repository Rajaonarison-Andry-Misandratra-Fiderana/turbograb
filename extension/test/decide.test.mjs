// The interception rules, tested outside a browser.
//
// `decide()` is where a wrong answer is expensive in both directions: take a
// download that should have stayed in the browser and the user loses a
// session-gated file; skip one that should have been taken and the extension
// looks broken. It is deliberately pure so it can be run here, with no browser
// and no app.
//
// Run: node --test extension/test/

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { strict as assert } from "node:assert";
import test from "node:test";
import vm from "node:vm";

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(join(here, "..", "common.js"), "utf8");
// `URL` and the timers are the only globals common.js touches at load time;
// `chrome`/`browser` are deliberately absent, which is what makes the module
// prove it degrades outside a browser.
const context = vm.createContext({ module: { exports: {} }, URL, setTimeout, clearTimeout });
vm.runInContext(source, context);
const TG = context.module.exports;
const D = TG.DEFAULTS;

const big = { url: "https://cdn.example.test/file.zip", fileSize: 50e6, filename: "file.zip" };

test("a normal large download is taken", () => {
  assert.equal(TG.decide(big, D), "");
});

test("the master switch wins over everything", () => {
  assert.equal(TG.decide(big, { ...D, enabled: false }), "disabled");
});

test("only http(s) can be handed over", () => {
  // A blob: or data: body lives in the page — the app could never re-fetch it,
  // so cancelling the browser's download would simply lose the file.
  for (const url of ["blob:https://x.test/abc", "data:text/plain,hi", "file:///tmp/x.zip"]) {
    assert.equal(TG.decide({ ...big, url }, D), "not-http");
  }
});

test("loopback stays in the browser", () => {
  // Including our own API: a download manager pulling from 127.0.0.1 over eight
  // connections is pure overhead.
  assert.equal(TG.decide({ ...big, url: "http://127.0.0.1:8787/downloads" }, D), "local");
  assert.equal(TG.decide({ ...big, url: "http://localhost:3000/build.zip" }, D), "local");
});

test("small files are left alone, and the threshold is honoured", () => {
  assert.equal(TG.decide({ ...big, fileSize: 200_000 }, D), "too-small");
  assert.equal(TG.decide({ ...big, fileSize: 200_000 }, { ...D, minSizeMB: 0.1 }), "");
});

test("an unknown size is taken by default and skippable by choice", () => {
  // Firefox often reports fileSize -1 at creation time; refusing those would
  // mean intercepting almost nothing.
  const unknown = { ...big, fileSize: -1 };
  assert.equal(TG.decide(unknown, D), "");
  assert.equal(TG.decide(unknown, { ...D, takeUnknownSize: false }), "unknown-size");
});

test("host exclusions match subdomains, not substrings", () => {
  const s = { ...D, skipHosts: "example.test\nintranet.local" };
  assert.equal(TG.decide(big, s), "skip-host");
  assert.equal(TG.decide({ ...big, url: "https://example.test/f.zip" }, s), "skip-host");
  // "notexample.test" merely ends with the same letters — it is a different site.
  assert.equal(TG.decide({ ...big, url: "https://notexample.test/f.zip" }, s), "");
});

test("extension exclusions ignore case and a leading dot", () => {
  const s = { ...D, skipExts: ".ZIP, torrent" };
  assert.equal(TG.decide(big, s), "skip-ext");
  assert.equal(TG.decide({ ...big, url: "https://x.test/a.torrent", filename: "a.torrent" }, s), "skip-ext");
  assert.equal(TG.decide({ ...big, url: "https://x.test/a.iso", filename: "a.iso" }, s), "");
});

test("a Windows-shaped path still yields its extension", () => {
  // Chrome reports `filename` as an OS path; a naive split on "/" keeps the
  // whole thing and every extension rule then misses.
  assert.equal(TG.baseName("C:\\Users\\me\\Downloads\\a.iso"), "a.iso");
  assert.equal(TG.extOf("a.tar.gz"), "gz");
});
