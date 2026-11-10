// End-to-end smoke harness: the real desktop UI (App.svelte) with the Tauri backend replaced by a
// mock that records every command and streams synthetic snapshot frames. Driven by
// scripts/e2e-smoke.mjs. Dev-only: not part of the production bundle.

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { FrameEvent, Seat, SessionInfo, Viewer } from "../lib/bindings";

declare global {
  interface Window {
    __ipcCalls: { cmd: string; args: unknown }[];
  }
}
window.__ipcCalls = [];

const seats: Seat[] = [
  { id: 0, name: "White cell", role: "white_cell" },
  { id: 1, name: "Blue", role: "side", side: 1 },
  { id: 2, name: "Neutral shipping", role: "side", side: 2 },
];
let seatId = 1;
let viewer: Viewer = { kind: "side", side: 1 };
const seatViewer = (s: Seat): Viewer =>
  s.role === "white_cell" ? { kind: "white_cell" } : { kind: "side", side: s.side };
let paused = true;
let tick = 0;
/** Set while "reviewing" a replay; the live tick is kept in `liveTick`. */
let replay: { end_s: number; verified: boolean } | null = null;
let liveTick = 0;
const aarReport = () => ({
  end_tick: tick,
  sides: [
    {
      side: 1,
      name: "Blue",
      units_start: 1,
      reinforcements: 0,
      destroyed: 0,
      removed: 0,
      remaining: 1,
      weapons_fired: 2,
      hits: 1,
      kills: 0,
    },
  ],
  losses: [
    {
      tick: 40,
      unit: 2,
      name: "Bulk carrier Bravo",
      side: 2,
      side_name: "Neutral shipping",
      platform: "GEN-MV-BULK",
      removed: false,
      cause: "white cell: Ruled sunk by umpire",
      by_side: null,
    },
  ],
  weapons: [],
  detections: [],
  timeline: [
    { tick: 12, kind: "order" as const, side: 1, text: "Blue: routed Frigate Alpha (2 waypoints)" },
    { tick: 40, kind: "loss" as const, side: 2, text: "Bulk carrier Bravo destroyed" },
  ],
});
let refusals: string[] = [];
const adjudications: {
  tick: number;
  umpire: string;
  reason: string;
  action: { kind: "destroy_unit"; unit: number };
}[] = [];
let emcon: "active" | "silent" | null = null;
type MockMessage = {
  tick: number;
  from: string;
  to: { to: "all" } | { to: "side"; side: number } | { to: "white_cell" };
  text: string;
  title: string | null;
  inject: string | null;
};
const messages: MockMessage[] = [
  {
    tick: 0,
    from: "Exercise control",
    to: { to: "all" },
    text: "STARTEX: proceed",
    title: null,
    inject: null,
  },
];
const injects = [
  {
    inject: {
      id: "i01-distress",
      title: "Distress call",
      from: "MV Ember",
      to: "side" as const,
      side: 1,
      text: "Engine room fire, request assistance",
      note: "Tests whether Blue diverts",
    },
    sent: 0,
  },
];
const scenarioEvents = [
  { id: "e01-startex", name: "Exercise control", enabled: true, repeatable: false, fired: 1 },
];

const session = (): SessionInfo => ({
  scenario_name: "E2E — mock scenario",
  description: "",
  sides: [
    { id: 1, name: "Blue" },
    { id: 2, name: "Neutral shipping" },
  ],
  viewer,
  seat: seats.find((s) => s.id === seatId) ?? { id: 0, name: "White cell", role: "white_cell" },
  seats: [...seats],
  locked: false,
  controls_time: true,
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
      case "order_adjudicate": {
        const a = args as { reason: string; action: { kind: "destroy_unit"; unit: number } };
        adjudications.push({ tick, umpire: "local", reason: a.reason, action: a.action });
        return null;
      }
      case "order_facilitate": {
        const a = (args as { action: { kind: string; inject?: string } }).action;
        const s = injects.find((x) => x.inject.id === a.inject);
        if (a.kind === "send_inject" && s) {
          s.sent += 1;
          messages.push({
            tick,
            from: s.inject.from,
            to: { to: s.inject.to, side: s.inject.side },
            text: s.inject.text,
            title: s.inject.title,
            inject: s.inject.id,
          });
        }
        return null;
      }
      case "order_engage":
        refusals = ["C1 track error 900 m exceeds the 500 m WPN-SAM-MR needs"];
        return null;
      case "order_doctrine":
        emcon = (args as { doctrine: { emcon: "active" | "silent" | null } }).doctrine.emcon;
        return null;
      case "set_seat": {
        const s = seats.find((x) => x.id === (args as { seat: number }).seat);
        if (s) {
          seatId = s.id;
          viewer = seatViewer(s);
        }
        return session();
      }
      case "add_seat": {
        const a = args as { name: string; side: number | null };
        const id = Math.max(...seats.map((s) => s.id)) + 1;
        seats.push(
          a.side === null
            ? { id, name: a.name, role: "white_cell" }
            : { id, name: a.name, role: "side", side: a.side },
        );
        return session();
      }
      case "remove_seat": {
        const i = seats.findIndex((s) => s.id === (args as { seat: number }).seat);
        if (i >= 0) seats.splice(i, 1);
        return session();
      }
      case "open_seat_window":
        return null;
      case "review_exercise":
        liveTick = tick;
        replay = { end_s: tick, verified: true };
        tick = 0;
        paused = true;
        return null;
      case "seek_replay":
        tick = Math.min((args as { tick: number }).tick, replay?.end_s ?? 0);
        return null;
      case "close_review":
        replay = null;
        tick = liveTick;
        return null;
      case "save_record":
        return [1, 2, 3];
      case "get_aar":
        return { report: aarReport(), replaying: replay !== null };
      case "aar_csv":
        return "time,tick,kind,side,text\n";
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

const frame = (): FrameEvent => {
  const sent = refusals;
  refusals = [];
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
      group: null,
      doctrine: {
        wcs: "safe" as const,
        emcon: emcon ?? ("active" as const),
        engage_unfriendly: false,
      },
      doctrine_set: { wcs: null, emcon, engage_unfriendly: null },
      status: "operational" as const,
      damage_fraction: 0,
      criticals: [],
      fires: 0,
      flooding: false,
      mission: null,
      mission_complete: false,
      stores: [
        { weapon: "WPN-ASM-SUB", count: 8 },
        { weapon: "WPN-SAM-MR", count: 32 },
      ],
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
      group: null,
      doctrine: { wcs: "tight" as const, emcon: "active" as const, engage_unfriendly: false },
      doctrine_set: { wcs: null, emcon: null, engage_unfriendly: null },
      status: "operational" as const,
      damage_fraction: 0,
      criticals: [],
      fires: 0,
      flooding: false,
      mission: null,
      mission_complete: false,
      stores: [],
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
            affiliation: null,
            category: "surface_combatant",
            platform: null,
            first_seen_s: 0,
            last_seen_s: tick,
            detected_by: viewer.side === 1 ? 1 : 2,
            sensor_id: "SEN-RDR-3D-MR",
          },
        ]
      : [];
  const bearings =
    viewer.kind === "side" && viewer.side === 1
      ? [
          {
            side: 1,
            observer: 1,
            line: { from: { lat_deg: -39.3, lon_deg: 146.0 }, bearing_deg: 270, sigma_deg: 2 },
            age_s: 5,
            emitter: "SEN-RDR-NAV",
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
      replay,
    },
    // The mock always sends full updates; diffs are covered by the backend's own tests.
    update: {
      full: true,
      tick,
      game_time_s: tick,
      viewer,
      units,
      removed: [],
      contacts,
      environment: {
        sea_state: 3,
        weather_loss_db: 0,
        start_unix_s: 1794268800,
        visibility_km: 40,
      },
      bearings,
      weapons: [],
      minefields: [],
      adjudications,
      messages,
      injects: viewer.kind === "white_cell" ? injects : [],
      scenario_events: viewer.kind === "white_cell" ? scenarioEvents : [],
      refusals: sent,
    },
  };
};

await import("../main");
setInterval(() => {
  if (!paused && (!replay || tick < replay.end_s)) tick += 1;
  void emit("frame-event", frame());
}, 100);
