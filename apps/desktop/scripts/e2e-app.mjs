// End-to-end test of the packaged desktop app: the real binary, the real engine, the embedded
// platform database and the bundled map, driven over WebDriver through tauri-driver.
// Complements e2e-smoke.mjs, which tests the UI against a mocked backend.
//
// Needs (docs/build/releasing.md, "Packaged-app test"):
//   - a release build of the app: `npx tauri build --no-bundle` (or a full build);
//   - tauri-driver on the PATH (`cargo install tauri-driver --locked`), or TAURI_DRIVER=<path>;
//   - msedgedriver matching the WebView2 version, on the PATH or in NATIVE_DRIVER.
//     macOS has no WebDriver for its webview, so this test runs on Windows.
// Run: npm run e2e:app   (NWS_APP=<path to the binary> to test another build)
//
// The W3C WebDriver protocol is spoken directly over fetch, so the test adds no dependencies.

import { spawn } from "node:child_process";
import { Buffer } from "node:buffer";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
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

// E2E_SHOTS=<folder>: save a screenshot at each rehearsal step, for review.
const shots = process.env.E2E_SHOTS ? resolve(process.env.E2E_SHOTS) : null;
if (shots) mkdirSync(shots, { recursive: true });
let session;
async function shot(name) {
  if (!shots || !session) return;
  const png = await wd("GET", `/session/${session}/screenshot`);
  writeFileSync(join(shots, `${name}.png`), Buffer.from(png, "base64"));
}

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
  // A cold start can take a while.
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
  // The starting picture arrives even when the window starts slowly (it asks for it once it
  // is listening).
  await until("units in the starting picture", async () => /Units [1-9]/.test(await bodyText()));
  check(true, "starting picture lists the seat's units");
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
  // 2a. Editing a record's JSON source: a live check against the whole database, a diff, and
  //     no save while the edit does not load.
  await click(await find("xpath", '//button[.//span[starts-with(normalize-space(),"GEN-OPV ·")]]'));
  await click(await until("the record detail", () => byText("button", "Edit record")));
  // Typing JSON (spaces, digits, dots) must not reach the run/pause and compression shortcuts.
  const json = await until("the record source", async () => {
    const v = await wd("GET", `${s}/element/${await el("#record-json")}/property/value`);
    return v.includes('"GEN-OPV"') ? v : false;
  });
  const setJson = async (t) => {
    const id = await el("#record-json");
    await wd("POST", `${s}/element/${id}/clear`, {});
    await wd("POST", `${s}/element/${id}/value`, { text: t });
  };
  await setJson(json.replace('"max_speed_kn": 21', '"max_speed_kn": 22'));
  // The unedited record also "checks pass", so wait for the diff of this edit first.
  const changes = await until("the diff of the edit", async () => {
    const t = await text(await el('[aria-label="Changes from the original"]'));
    return t.includes('"max_speed_kn": 22') ? t : false;
  });
  await until("the edit to check clean", async () => {
    const b = await bodyText();
    return b.includes("Checks pass") && !b.includes("Checking against");
  });
  check(
    changes.includes('"max_speed_kn": 21'),
    "record editor checks an edit clean and shows its diff",
  );
  const typed = await wd("GET", `${s}/element/${await el("#record-json")}/property/value`);
  check(
    typed.length === json.length && (await clock()) === "D+0 00:00:01",
    "typing in the editor keeps every character and leaves the clock alone",
  );
  await setJson(json.replace('"max_speed_kn"', '"max_sped_kn"'));
  await until("the bad edit to be reported", async () =>
    (await bodyText()).includes("Does not load"),
  );
  const saveEnabled = await wd(
    "GET",
    `${s}/element/${await byText("button", "Save to source file")}/enabled`,
  );
  check(!saveEnabled, "an edit that does not load is reported and cannot be saved");
  await shot("00-record-editor");
  await click(await byText("button", "Discard changes"));
  await click(await byText("button", "Close editor"));
  // Weapons (and sensors) open straight in the editor, from their own source file.
  await click(await byText("button", "Weapons"));
  await click(
    await until("the weapons list", () =>
      find("xpath", '//button[.//span[normalize-space()="WPN-GUN-30"]]'),
    ),
  );
  const weaponEditor = await until("the weapon editor", async () => {
    const t = await text(await el('[aria-label="Edit database record"]'));
    return t.includes("weapons.json") ? t : false;
  });
  check(
    weaponEditor.includes("Edit WPN-GUN-30"),
    "weapon records open in the editor from weapons.json",
  );
  await click(await byText("button", "Platforms"));
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

  // 4a. Rehearsal of the white-cell run sheet (docs/gate3/pilot-kit/white-cell-run-sheet.md),
  //     before STARTEX.
  const tabBtn = (name) =>
    find("xpath", `//button[@role="tab"][normalize-space()=${JSON.stringify(name)}]`);
  const all = async (css) =>
    (await wd("POST", `${s}/elements`, { using: "css selector", value: css })).map(
      (e) => Object.values(e)[0],
    );
  const typeInto = async (id, value) => {
    await wd("POST", `${s}/element/${id}/clear`, {});
    await wd("POST", `${s}/element/${id}/value`, { text: value });
  };
  const unread = async () => {
    const t = await text(
      await find("xpath", '//button[@role="tab"][starts-with(normalize-space(),"Messages")]'),
    );
    return Number(/Messages\s*(\d+)/.exec(t)?.[1] ?? 0);
  };
  await shot("01-white-cell-loaded");

  const tabs = [];
  for (const name of ["Unit", "Injects", "Events", "Planning", "Seats", "Log", "Review"]) {
    if (await tabBtn(name).catch(() => null)) tabs.push(name);
  }
  check(tabs.length === 7, `white-cell console tabs present (${tabs.join(", ")})`);

  await click(await tabBtn("Events"));
  const events = await until("the events list", async () => {
    const n = (await all('[aria-label="Scenario events"] > li')).length;
    return n || false;
  });
  check(events === 8, `Events tab lists the scenario's 8 events (${events})`);

  await click(await tabBtn("Injects"));
  const injects = (await all('[aria-label="Injects"] > li')).length;
  check(injects === 4, `Injects tab lists the 4 injects (${injects})`);
  const before = await unread();
  await click(await el('[aria-label="Send Jamming warning"]'));
  const after = await until("the inject to arrive", async () => {
    const n = await unread();
    return n > before ? n : false;
  });
  check(after > before, `sending an inject delivers it (unread ${before} → ${after})`);
  await shot("02-inject-sent");

  await click(await tabBtn("Planning"));
  const posts = await text(await el('[aria-label="Command posts"]'));
  check(
    posts.includes("Blue flagship command") &&
      posts.includes("Blue maritime HQ") &&
      !/down/.test(posts),
    "Planning tab shows both Blue command posts up",
  );
  await shot("03-planning");

  // A seat window opens, locked to its seat, showing only its side.
  await click(await tabBtn("Seats"));
  const main = await wd("GET", `${s}/window`);
  await click(await el('[aria-label="Open a window for Blue command"]'));
  const blue = await until("the Blue seat window", async () => {
    const handles = await wd("GET", `${s}/window/handles`);
    return handles.find((h) => h !== main) ?? false;
  });
  await wd("POST", `${s}/window`, { handle: blue });
  const blueText = await until("Blue units in the seat window", async () => {
    const b = await bodyText();
    return b.includes("Blue destroyer (DDG) — Anvil") ? b : false;
  });
  check(
    !blueText.includes("Red corvette (FFL) — Cinder") && blueText.includes("Blue command"),
    "Blue seat window is locked to Blue and hides Red units",
  );
  await sleep(3000); // let the seat window's map finish loading its coastline
  await shot("04-blue-seat-window");
  await wd("DELETE", `${s}/window`);
  await wd("POST", `${s}/window`, { handle: main });

  // An override needs a reason, and is logged with it.
  await typeInto(await el("#umpire-reason"), "rehearsal: test override");
  await click(
    await find(
      "xpath",
      '//button[.//span[@class="name" and text()="Red patrol boat (PGG) — Flint"]]',
    ),
  );
  await click(await tabBtn("Unit"));
  await click(await byText("button", "Destroy"));
  await click(await tabBtn("Log"));
  const log = await until("the override in the log", async () => {
    const t = await bodyText();
    return t.includes("rehearsal: test override") ? t : false;
  });
  check(/destroy unit/.test(log), "override is logged with the umpire's reason");
  await shot("05-override-log");

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
  await shot("06-after-run");

  // 6. Hot wash: the review replays the exercise so far and verifies it, then back to live.
  await click(await tabBtn("Review"));
  await click(await byText("button", "Replay"));
  await until("the verified replay", async () =>
    (await bodyText()).includes("Replay: matches the recorded run"),
  );
  check(true, "review replays the exercise and verifies it against the record");
  await shot("07-review");
  await click(await byText("button", "Back to live"));
  await until("live again", async () => !(await bodyText()).includes("REPLAY"));
  check(true, "back to live after the review");
} catch (e) {
  check(false, String(e.message ?? e));
} finally {
  if (session) await wd("DELETE", `/session/${session}`).catch(() => {});
  driver.kill();
}

console.log(failures.length ? `\n${failures.length} check(s) failed` : "\nall checks passed");
process.exit(failures.length ? 1 : 0);
