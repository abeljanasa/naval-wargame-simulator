// Map rendering benchmark (master prompt §10/§12: 60 fps with 10,000 symbols).
// Uses the production map code (createMap/buildLayers) with N synthetic unit symbols and labels,
// rebuilds the layers at 30 Hz with moving positions (as snapshot frames do in the app), and
// measures the browser frame rate. Query: ?units=10000&seconds=10
// The result is exposed as window.__mapFps for scripts/map-fps.mjs.

import { buildLayers, createMap } from "../lib/map";
import type { UnitView } from "../lib/bindings";

const params = new URLSearchParams(location.search);
const N = Number(params.get("units") ?? 10000);
const SECONDS = Number(params.get("seconds") ?? 10);
/** 0 disables the 30 Hz snapshot updates (pure rendering cost). */
const UPDATE = params.get("update") !== "0";
const WARMUP_MS = 5000;

interface Result {
  units: number;
  seconds: number;
  fps: number;
  p50FrameMs: number;
  p95FrameMs: number;
  p99FrameMs: number;
  maxFrameMs: number;
  /** Frames longer than 50 ms (visible stutter). */
  longFrames: number;
  renderer: string;
}
declare global {
  interface Window {
    __mapFps?: Result;
  }
}

// Deterministic placement around the map centre (mulberry32).
let seed = 1;
const rand = () => {
  seed |= 0;
  seed = (seed + 0x6d2b79f5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};

let units: UnitView[] = Array.from({ length: N }, (_, i) => ({
  id: i + 1,
  name: `Unit ${i + 1}`,
  side: (i % 2) + 1,
  domain: "surface",
  position: { lat_deg: -39.3 + (rand() - 0.5) * 6, lon_deg: 146 + (rand() - 0.5) * 9 },
  altitude_m: 0,
  heading_deg: rand() * 360,
  speed_mps: 8,
  ordered_speed_mps: 8,
  max_speed_mps: 15,
  platform: "GEN-FFG",
  route: null,
}));

function rendererName(): string {
  const gl = document.createElement("canvas").getContext("webgl2");
  const ext = gl?.getExtension("WEBGL_debug_renderer_info");
  return gl && ext ? String(gl.getParameter(ext.UNMASKED_RENDERER_WEBGL)) : "unknown";
}

const status = document.getElementById("status");
const container = document.getElementById("map");
if (!container || !status) throw new Error("benchmark page is missing elements");
const { map, overlay } = createMap(container, [146.0, -39.3]);
map.setZoom(5.5);

const redraw = () =>
  overlay.setProps({
    layers: buildLayers({
      units,
      contacts: [],
      gameTimeS: 0,
      viewer: { kind: "white_cell" },
      selected: null,
      draft: null,
      onPickUnit: () => {},
    }),
  });

map.on("load", () => {
  redraw();
  // 30 Hz snapshot stream: move every unit a little and rebuild the layers.
  // Like the app, each snapshot is a fresh array of fresh unit objects.
  if (UPDATE) {
    setInterval(() => {
      units = units.map((u) => {
        const h = ((u.heading_deg ?? 0) * Math.PI) / 180;
        return {
          ...u,
          position: {
            lat_deg: (u.position.lat_deg ?? 0) + Math.cos(h) * 1e-4,
            lon_deg: (u.position.lon_deg ?? 0) + Math.sin(h) * 1e-4,
          },
        };
      });
      redraw();
    }, 33);
  }

  const frames: number[] = [];
  let last = performance.now();
  const start = last + WARMUP_MS;
  const tick = (now: number) => {
    if (now >= start) frames.push(now - last);
    last = now;
    if (now < start + SECONDS * 1000) {
      requestAnimationFrame(tick);
      return;
    }
    const sorted = [...frames].sort((a, b) => a - b);
    const result: Result = {
      units: N,
      seconds: SECONDS,
      fps: (frames.length * 1000) / frames.reduce((a, b) => a + b, 0),
      p50FrameMs: sorted[Math.floor(sorted.length * 0.5)] ?? Number.NaN,
      p95FrameMs: sorted[Math.floor(sorted.length * 0.95)] ?? Number.NaN,
      p99FrameMs: sorted[Math.floor(sorted.length * 0.99)] ?? Number.NaN,
      maxFrameMs: sorted[sorted.length - 1] ?? Number.NaN,
      longFrames: frames.filter((f) => f > 50).length,
      renderer: rendererName(),
    };
    window.__mapFps = result;
    status.textContent = `${N} symbols: ${result.fps.toFixed(1)} fps (p95 frame ${result.p95FrameMs.toFixed(1)} ms) on ${result.renderer}`;
  };
  status.textContent = `measuring ${N} symbols for ${SECONDS} s…`;
  requestAnimationFrame(tick);
});
