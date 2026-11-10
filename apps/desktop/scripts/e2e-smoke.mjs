// End-to-end smoke test of the desktop UI against a mocked backend (src/e2e/mock-backend.ts).
// Checks that the real UI loads a session, shows units and contacts, and sends the right commands
// for pause/run, time compression, seat changes and route planning.
// Run: npm run e2e   (headless; set E2E_HEADED=1 to watch)

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
  check(await page.getByText(/C1 · unknown/).isVisible(), "contact C1 shown in the Contacts panel");

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

  await page.getByLabel("Seat").selectOption("white");
  await page.getByRole("button", { name: /Bulk carrier Bravo/ }).waitFor({ timeout: 5_000 });
  check(
    (await calls()).some((c) => c.cmd === "set_viewer" && c.args.viewer.kind === "white_cell"),
    "white-cell seat sends set_viewer and shows every unit",
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
