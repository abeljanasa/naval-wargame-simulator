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

const platforms = [
  {
    id: "GEN-FFG",
    name: "Generic frigate",
    category: "frigate",
    domain: "surface",
    max_speed_mps: 14.4,
  },
  {
    id: "GEN-MV-BULK",
    name: "Generic bulk carrier",
    category: "merchant",
    domain: "surface",
    max_speed_mps: 7.7,
  },
];

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
      case "db_search": {
        const q = (args as { query: string }).query.toLowerCase();
        return platforms.filter(
          (p) => !q || `${p.id} ${p.name} ${p.category}`.toLowerCase().includes(q),
        );
      }
      case "db_platform": {
        const p = platforms.find((x) => x.id === (args as { id: string }).id);
        if (!p) throw `no platform ${(args as { id: string }).id}`;
        return {
          row: p,
          cruise_speed_mps: 9,
          accel_mps2: 0.1,
          length_m: 130,
          rcs_m2: 3000,
          sensors: [
            { sensor_id: "SEN-RDR-3D-MR", kind: "radar", mount_height_m: 25, max_range_m: 180000 },
          ],
          source_citation: "Generic open-source figure (mock)",
          confidence: "medium",
          classification: "UNCLASSIFIED",
        };
      }
      case "db_issues":
        return [
          {
            severity: "warning",
            rule: "W-MOCK",
            location: "platforms/GEN-FFG",
            message: "mock warning",
          },
        ];
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
            error: { semi_major_m: 500, semi_minor_m: 60, orientation_deg: 0 },
            classification: "domain" as const,
            domain: "surface" as const,
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
    // The mock always sends full updates; diffs are covered by the backend's own tests.
    update: { full: true, tick, game_time_s: tick, viewer, units, removed: [], contacts },
  };
};

await import("../main");
setInterval(() => {
  if (!paused) tick += 1;
  void emit("frame-event", frame());
}, 100);
