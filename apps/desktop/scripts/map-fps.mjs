// Runs the map FPS benchmark page in a real Chromium browser and checks the budget.
// Usage: node scripts/map-fps.mjs [--units 10000] [--seconds 10] [--min-fps 60] [--headed]
// On Windows it uses Microsoft Edge, the same engine as the app's WebView2. Elsewhere it uses
// Chrome if installed, or Playwright's Chromium. The GPU renderer is reported: software rendering
// (SwiftShader/llvmpipe) does not represent the reference laptop, so the budget is not enforced there.

import { existsSync } from "node:fs";
import process from "node:process";
import { createServer } from "vite";
import { chromium } from "playwright";

const arg = (name, dflt) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : dflt;
};
const units = Number(arg("units", 10000));
const seconds = Number(arg("seconds", 10));
const minFps = Number(arg("min-fps", 60));
const headed = process.argv.includes("--headed");

const server = await createServer({ server: { port: 1431, strictPort: true }, logLevel: "error" });
await server.listen();

// Prefer an installed system browser (Edge = the WebView2 engine on Windows); no download needed.
const candidates =
  {
    win32: [
      "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
      "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
      "C:/Program Files/Google/Chrome/Application/chrome.exe",
    ],
    darwin: ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"],
    linux: ["/usr/bin/google-chrome", "/usr/bin/chromium", "/usr/bin/chromium-browser"],
  }[process.platform] ?? [];
const executablePath = candidates.find((p) => existsSync(p));
const browser = await chromium.launch({
  executablePath, // undefined → Playwright's own Chromium (`npx playwright install chromium`)
  headless: !headed,
  args: ["--ignore-gpu-blocklist"],
});
let code = 0;
try {
  const page = await browser.newPage({ viewport: { width: 1600, height: 900 } });
  await page.goto(
    `http://localhost:1431/bench/map-fps.html?units=${units}&seconds=${seconds}&update=${arg("update", "1")}`,
  );
  await page.waitForFunction(() => window.__mapFps, null, { timeout: (seconds + 60) * 1000 });
  const r = await page.evaluate(() => window.__mapFps);
  const software = /swiftshader|llvmpipe|software|basic render/i.test(r.renderer);
  const passed = r.fps >= minFps * 0.95; // vsync-locked 60 Hz displays report ~59.x
  console.log(
    JSON.stringify({ ...r, browser: browser.version(), headed, software, minFps, passed }),
  );
  if (software) {
    console.log("software WebGL renderer: result recorded, budget not enforced");
  } else if (!passed) {
    console.error(`map budget missed: ${r.fps.toFixed(1)} fps < ${minFps} fps`);
    code = 1;
  }
} finally {
  await browser.close();
  await server.close();
}
process.exit(code);
