// End-to-end smoke test of the desktop UI against a mocked backend (src/e2e/mock-backend.ts).
// Checks that the real UI loads a session, shows units and contacts, and sends the right commands
// for pause/run, time compression, seat changes and route planning.
// Run: npm run e2e   (headless; set E2E_HEADED=1 to watch)

import { Buffer } from "node:buffer";
import { existsSync } from "node:fs";
import process from "node:process";
import { createServer } from "vite";
import { chromium } from "playwright";

const server = await createServer({ server: { port: 1432, strictPort: true }, logLevel: "error" });
await server.listen();
const executablePath = [
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
  "/usr/bin/google-chrome",
].find((p) => existsSync(p));
const browser = await chromium.launch({ executablePath, headless: !process.env.E2E_HEADED });

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

try {
  const page = await browser.newPage({ viewport: { width: 1400, height: 850 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.goto("http://localhost:1432/e2e/app.html");
  const calls = () => page.evaluate(() => window.__ipcCalls);

  await page.getByText("E2E — mock scenario").waitFor({ timeout: 30_000 });
  check(true, "session loads (scenario name in header)");
  await page.getByRole("button", { name: /Frigate Alpha/ }).waitFor();
  check(true, "own unit listed for the Blue seat");
  check(
    (await page.getByRole("button", { name: /Bulk carrier Bravo/ }).count()) === 0,
    "other side's unit hidden from the Blue seat",
  );
  check(
    await page.getByText(/C1 · surface combatant/).isVisible(),
    "contact C1 shown in the Contacts panel with its ESM category",
  );
  check(
    await page.getByText("± 1.2 km (95 %)", { exact: false }).isVisible(),
    "contact shows its 95 % position error",
  );

  check(await page.getByText("Sea state 3").isVisible(), "header shows the sea state");

  await page.keyboard.press("Space");
  await page.waitForTimeout(300);
  check(
    (await calls()).some((c) => c.cmd === "set_paused" && c.args.paused === false),
    "Space sends set_paused(false)",
  );

  await page.getByRole("button", { name: "60×" }).click();
  check(
    (await calls()).some((c) => c.cmd === "set_time_compression" && c.args.compression === "x60"),
    "60× button sends set_time_compression(x60)",
  );

  await page.getByRole("button", { name: /Frigate Alpha/ }).click();
  await page.getByRole("button", { name: /Plan new route/ }).click();
  const map = page.locator("main.map");
  const box = await map.boundingBox();
  await map.click({ position: { x: box.width * 0.3, y: box.height * 0.6 } });
  await map.click({ position: { x: box.width * 0.4, y: box.height * 0.65 } });
  await page.getByText(/2 so far/).waitFor({ timeout: 5_000 });
  await page.keyboard.press("Enter");
  await page.waitForTimeout(300);
  const route = (await calls()).find((c) => c.cmd === "order_route");
  check(
    route && route.args.unit === 1 && route.args.waypoints.length === 2,
    "map clicks + Enter send order_route with 2 waypoints",
  );

  await page.getByLabel("Emissions").selectOption("silent");
  await page.waitForTimeout(300);
  check(
    (await calls()).some(
      (c) => c.cmd === "order_doctrine" && c.args.unit === 1 && c.args.doctrine.emcon === "silent",
    ),
    "emissions selector sends order_doctrine(emcon silent)",
  );
  check(
    (await page.getByLabel("Weapons").inputValue()) === "inherit" &&
      (await page.getByRole("option", { name: "Inherit (safe)" }).count()) === 1,
    "weapons selector shows the inherited WCS",
  );

  await page.getByLabel("Weapon", { exact: true }).selectOption("WPN-SAM-MR");
  await page.getByRole("button", { name: "Fire" }).click();
  await page.waitForTimeout(300);
  check(
    (await calls()).some(
      (c) =>
        c.cmd === "order_engage" &&
        c.args.shooter === 1 &&
        c.args.contact === 1 &&
        c.args.weapon === "WPN-SAM-MR" &&
        c.args.rounds === 1,
    ),
    "engage form sends order_engage(shooter 1, C1, WPN-SAM-MR, 1 round)",
  );
  await page.getByText(/exceeds the 500 m WPN-SAM-MR needs/).waitFor({ timeout: 5_000 });
  check(true, "an engine refusal of the order is shown to the player");

  await page.getByRole("button", { name: "Patrol here" }).click();
  await page.waitForTimeout(300);
  check(
    (await calls()).some(
      (c) => c.cmd === "order_mission" && c.args.unit === 1 && c.args.mission?.kind === "patrol",
    ),
    "Patrol here sends order_mission(patrol)",
  );

  await page.getByLabel("Scenario").setInputFiles({
    name: "loaded.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify({ format: "nws-scenario", name: "Loaded from file" })),
  });
  await page.getByText("Loaded from file").waitFor({ timeout: 5_000 });
  check(
    (await calls()).some(
      (c) => c.cmd === "load_scenario" && c.args.json.includes("Loaded from file"),
    ),
    "picking a scenario file sends load_scenario and shows the new scenario",
  );

  await page.getByLabel("Seat").selectOption("white");
  await page.getByRole("button", { name: /Bulk carrier Bravo/ }).waitFor({ timeout: 5_000 });
  check(
    (await calls()).some((c) => c.cmd === "set_viewer" && c.args.viewer.kind === "white_cell"),
    "white-cell seat sends set_viewer and shows every unit",
  );

  await page.getByText("Platform database", { exact: true }).first().click();
  await page.getByPlaceholder("Search id, name or category").fill("bulk");
  await page.getByRole("button", { name: /Generic bulk carrier/ }).waitFor({ timeout: 5_000 });
  check(
    (await page.getByRole("list", { name: "Platforms" }).getByRole("button").count()) === 1,
    "database search filters the platform list",
  );
  await page.getByPlaceholder("Search id, name or category").fill("");
  await page.getByRole("button", { name: /Generic frigate/ }).click();
  await page.getByText("Generic open-source figure (mock)").waitFor({ timeout: 5_000 });
  check(
    await page.getByText("W-MOCK: mock warning").isVisible(),
    "platform record shows provenance and its validation warning",
  );

  check(errors.length === 0, `no page errors${errors.length ? `: ${errors.join("; ")}` : ""}`);
} catch (e) {
  failures.push(String(e));
  console.error(e);
} finally {
  await browser.close();
  await server.close();
}
console.log(
  failures.length ? `${failures.length} check(s) failed` : "e2e smoke: all checks passed",
);
process.exit(failures.length ? 1 : 0);
