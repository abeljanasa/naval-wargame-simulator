// End-to-end test of the packaged desktop app: the real binary, the real engine, the embedded
// platform database and the bundled map, driven over WebDriver through tauri-driver.
// Complements e2e-smoke.mjs, which tests the UI against a mocked backend.
//
// Needs (docs/build/releasing.md, "Packaged-app test"):
//   - a release build of the app: `npx tauri build --no-bundle` (or a full build);
//   - tauri-driver on the PATH (`cargo install tauri-driver --locked`), or TAURI_DRIVER=<path>;
//   - the platform's native driver: msedgedriver matching the WebView2 version (Windows), or
//     WebKitWebDriver (Linux, package webkit2gtk-driver). Pass its path in NATIVE_DRIVER if it is
//     not on the PATH. macOS has no WKWebView driver, so this test runs on Windows and Linux.
// Run: npm run e2e:app   (NWS_APP=<path to the binary> to test another build)
//
// The W3C WebDriver protocol is spoken directly over fetch, so the test adds no dependencies.

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import process from "node:process";
import { setTimeout as sleep } from "node:timers/promises";

const root = resolve(import.meta.dirname, "../../..");
const exe = process.platform === "win32" ? ".exe" : "";
const app = resolve(process.env.NWS_APP ?? `${root}/target/release/nws-desktop${exe}`);
const pilot = resolve(root, "data/scenarios/phase3-pilot.json");
if (!existsSync(app)) {
  console.error(`No app binary at ${app}. Build it first: npx tauri build --no-bundle`);
  process.exit(2);
}

const PORT = 4444;
const base = `http://127.0.0.1:${PORT}`;
const driverArgs = ["--port", String(PORT)];
if (process.env.NATIVE_DRIVER) driverArgs.push("--native-driver", process.env.NATIVE_DRIVER);
const driver = spawn(process.env.TAURI_DRIVER ?? "tauri-driver", driverArgs, {
  stdio: ["ignore", "inherit", "inherit"],
});

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

async function wd(method, path, body) {
  const res = await fetch(base + path, {
    method,
    headers: { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const json = await res.json();
  if (!res.ok) throw new Error(`${method} ${path}: ${JSON.stringify(json.value ?? json)}`);
  return json.value;
}

/** Polls until `fn` returns a truthy value, or fails after `ms`. */
async function until(what, fn, ms = 30_000) {
  const end = Date.now() + ms;
  let last;
  while (Date.now() < end) {
    try {
      last = await fn();
      if (last) return last;
    } catch (e) {
      last = e;
    }
    await sleep(250);
  }
  throw new Error(`timed out waiting for ${what} (last: ${last})`);
}

let session;
try {
  await until("tauri-driver", async () => (await fetch(`${base}/status`)).ok, 20_000);
  session = (
    await wd("POST", "/session", {
      capabilities: { alwaysMatch: { browserName: "wry", "tauri:options": { application: app } } },
    })
  ).sessionId;
  const s = `/session/${session}`;
  const find = async (using, value) =>
    Object.values(await wd("POST", `${s}/element`, { using, value }))[0];
  const el = (css) => find("css selector", css);
  const byText = (tag, t) => find("xpath", `//${tag}[normalize-space()=${JSON.stringify(t)}]`);
  const click = async (id) => wd("POST", `${s}/element/${id}/click`, {});
  const text = async (id) => wd("GET", `${s}/element/${id}/text`);
  const bodyText = async () => text(await el("body"));
  const clock = async () => text(await el(".clock .time"));
  // "D+1 01:02:03" -> seconds.
  const seconds = (c) => {
    const m = /D\+(\d+) (\d+):(\d+):(\d+)/.exec(c);
    return m ? ((+m[1] * 24 + +m[2]) * 60 + +m[3]) * 60 + +m[4] : NaN;
  };

  // 1. The app starts on its built-in scenario, with the real engine behind it.
  // A cold start (first WebKitGTK launch under a virtual display) can take a while.
  await until(
    "the built-in scenario",
    async () => (await bodyText()).includes("Phase 1 — Strait transit"),
    90_000,
  ).catch(async (e) => {
    // Show what the page holds, to tell a blank render from a script error.
    const state = await wd("POST", `${s}/execute/sync`, {
      script: "return [document.readyState, document.title, document.body?.innerHTML.length ?? 0];",
      args: [],
    }).catch((x) => String(x));
    console.log("page state:", JSON.stringify(state));
    console.log("page text:", JSON.stringify((await bodyText().catch(() => "")).slice(0, 400)));
    throw e;
  });
  check(true, "app starts and shows the built-in scenario");
  // The host runs the first second on load, which places the units.
  check(seconds(await clock()) === 1, "opens after the first second (D+0 00:00:01)");

  // 2. The embedded platform database opens from the menu bar.
  await click(await byText("button", "View"));
  await click(await byText("button", "Platform database…"));
  const db = await until("the database dialog", async () => {
    const t = await text(await el('[aria-label="Platform database"]'));
    return t.includes("GEN-") ? t : false;
  });
  check(
    (db.match(/GEN-[A-Z0-9-]+/g) ?? []).length >= 10,
    "platform database lists generic records",
  );
  await click(await el('[aria-label="Close the database"]'));

  // 3. A scenario file loads through the File › Open picker (the hidden file input).
  await wd("POST", `${s}/element/${await el("#scenario-file")}/value`, { text: pilot });
  await until("the pilot scenario", async () => (await bodyText()).includes("Kent passage pilot"));
  check(true, "pilot scenario loads from a file");

  // 4. The white-cell seat (seat 1 in the pilot) sees ground truth before anything runs:
  //    Red units are listed while the clock is still paused.
  await click(await el('#seat option[value="1"]'));
  await until("Red units in the white-cell picture", async () =>
    (await bodyText()).includes("Red corvette (FFL) — Cinder"),
  );
  check(true, "white-cell seat shows the other side's units");

  // 5. The engine runs: at Max the clock moves on, and the scenario's events send messages.
  await click(await byText("button", "Max"));
  await click(await el(".toolbar .play"));
  const t = await until(
    "game time to pass ten minutes",
    async () => {
      const c = await clock();
      return seconds(c) > 600 ? c : false;
    },
    60_000,
  );
  check(true, `engine runs at Max (clock at ${t})`);
  await click(await el(".toolbar .play"));
  const tab = await text(
    await find("xpath", '//button[@role="tab"][starts-with(normalize-space(),"Messages")]'),
  );
  check(/Messages\s*\d+/.test(tab), `scenario events sent messages (tab reads "${tab}")`);
} catch (e) {
  check(false, String(e.message ?? e));
} finally {
  if (session) await wd("DELETE", `/session/${session}`).catch(() => {});
  driver.kill();
}

console.log(failures.length ? `\n${failures.length} check(s) failed` : "\nall checks passed");
process.exit(failures.length ? 1 : 0);
