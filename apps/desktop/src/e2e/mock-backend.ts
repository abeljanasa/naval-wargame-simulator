// End-to-end smoke harness: the real desktop UI (App.svelte) with the Tauri backend replaced by a
// mock that records every command and streams synthetic snapshot frames. Driven by
// scripts/e2e-smoke.mjs. Dev-only: not part of the production bundle.

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { FrameEvent, SessionInfo, Viewer } from "../lib/bindings";

declare global {
  interface Window {
    __ipcCalls: { cmd: string; args: unknown }[];
  }
}
window.__ipcCalls = [];

let viewer: Viewer = { kind: "side", side: 1 };
let paused = true;
let tick = 0;

const session = (): SessionInfo => ({
  scenario_name: "E2E — mock scenario",
  description: "",
  sides: [
    { id: 1, name: "Blue" },
    { id: 2, name: "Neutral shipping" },
  ],
  viewer,
});

mockWindows("main");
mockIPC(
  (cmd, args) => {
    if (!cmd.startsWith("plugin:")) window.__ipcCalls.push({ cmd, args });
    switch (cmd) {
      case "get_session":
        return session();
      case "set_paused":
        paused = (args as { paused: boolean }).paused;
        return null;
      case "load_scenario": {
        const parsed = JSON.parse((args as { json: string }).json) as { name: string };
        return { ...session(), scenario_name: parsed.name };
      }
      case "set_viewer":
        viewer = (args as { viewer: Viewer }).viewer;
        return null;
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

const frame = (): FrameEvent => {
  const units = [
    {
      id: 1,
      name: "Frigate Alpha",
      side: 1,
      domain: "surface" as const,
      position: { lat_deg: -39.3, lon_deg: 146.0 },
      altitude_m: 0,
      heading_deg: 250,
      speed_mps: 9,
      ordered_speed_mps: 9,
      max_speed_mps: 14.4,
      platform: "GEN-FFG",
      route: { waypoints: [{ lat_deg: -39.4, lon_deg: 145.5 }], next: 0 },
    },
    {
      id: 2,
      name: "Bulk carrier Bravo",
      side: 2,
      domain: "surface" as const,
      position: { lat_deg: -39.3, lon_deg: 145.6 },
      altitude_m: 0,
      heading_deg: 90,
      speed_mps: 6.5,
      ordered_speed_mps: 6.5,
      max_speed_mps: 7.7,
      platform: "GEN-MV-BULK",
      route: null,
    },
  ].filter((u) => viewer.kind === "white_cell" || u.side === viewer.side);
  const contacts =
    viewer.kind === "side"
      ? [
          {
            side: viewer.side,
            number: 1,
            position: { lat_deg: -39.3, lon_deg: viewer.side === 1 ? 145.6 : 146.0 },
            first_seen_s: 0,
            last_seen_s: tick,
            detected_by: viewer.side === 1 ? 1 : 2,
            sensor_id: "SEN-RDR-3D-MR",
          },
        ]
      : [];
  return {
    clock: {
      tick,
      game_time_s: tick,
      paused,
      compression: "x1",
      achieved_ratio: paused ? 0 : 1,
      scenario_duration_s: 86400,
    },
    snapshot: { protocol_version: 1, tick, game_time_s: tick, viewer, units, contacts },
  };
};

await import("../main");
setInterval(() => {
  if (!paused) tick += 1;
  void emit("frame-event", frame());
}, 100);
