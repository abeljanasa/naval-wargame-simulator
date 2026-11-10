// UI-side session state. It holds the latest frame from the engine plus purely local UI state
// (selection, draft route). Every change to the simulation goes through `commands`.

import {
  commands,
  events,
  type ClockStatus,
  type ContactView,
  type FrameEvent,
  type GeoPoint,
  type SessionInfo,
  type TimeCompression,
  type UnitView,
  type Viewer,
} from "./bindings";
import { MPS_PER_KNOT } from "./format";

export const COMPRESSIONS: { key: TimeCompression; label: string }[] = [
  { key: "x1", label: "1×" },
  { key: "x2", label: "2×" },
  { key: "x5", label: "5×" },
  { key: "x15", label: "15×" },
  { key: "x30", label: "30×" },
  { key: "x60", label: "60×" },
  { key: "max", label: "Max" },
];

class SimSession {
  session = $state<SessionInfo | null>(null);
  /** Latest clock from the engine. */
  clock = $state<ClockStatus | null>(null);
  /** The viewer's picture, kept up to date from full and diff updates. */
  units = $state<UnitView[]>([]);
  allContacts = $state<ContactView[]>([]);
  selected = $state<number | null>(null);
  /** Waypoints being planned for the selected unit; null when not planning. */
  draft = $state<GeoPoint[] | null>(null);
  error = $state<string | null>(null);

  /** Contacts of the viewing side. The white cell sees ground truth, so its view shows none. */
  contacts = $derived(this.session?.viewer.kind === "side" ? this.allContacts : []);
  selectedUnit = $derived(this.units.find((u) => u.id === this.selected) ?? null);

  async start() {
    this.session = await commands.getSession();
    await events.frameEvent.listen((e) => this.apply(e.payload));
  }

  /** Applies one frame: a full picture replaces the units; a diff upserts by id and removes. */
  apply(frame: FrameEvent) {
    const u = frame.update;
    this.clock = frame.clock;
    if (u.full) {
      this.units = u.units;
    } else if (u.units.length || u.removed.length) {
      const gone = new Set<number>([...u.removed, ...u.units.map((x) => x.id)]);
      this.units = [...this.units.filter((x) => !gone.has(x.id)), ...u.units].sort(
        (a, b) => a.id - b.id,
      );
    }
    this.allContacts = u.contacts;
  }

  /** Loads a scenario file the user picked; the engine validates it and resets the session. */
  async loadScenario(file: File) {
    const res = await commands.loadScenario(await file.text());
    if (res.status === "error") {
      this.error = `Could not load ${file.name}: ${res.error}`;
      return;
    }
    this.session = res.data;
    this.clock = null;
    this.units = [];
    this.allContacts = [];
    this.selected = null;
    this.draft = null;
    this.error = null;
  }

  async togglePause() {
    await commands.setPaused(!(this.clock?.paused ?? true));
  }

  async setCompression(c: TimeCompression) {
    await commands.setTimeCompression(c);
  }

  async step(ticks: number) {
    await commands.stepTicks(ticks);
  }

  async setViewer(v: Viewer) {
    this.cancelPlan();
    this.selected = null;
    await commands.setViewer(v);
    if (this.session) this.session.viewer = v;
  }

  select(id: number | null) {
    if (this.draft) return; // keep the selection stable while a route is being drawn
    this.selected = id;
  }

  beginPlan() {
    if (this.selectedUnit) this.draft = [];
  }

  addWaypoint(p: GeoPoint) {
    if (this.draft) this.draft = [...this.draft, p];
  }

  undoWaypoint() {
    if (this.draft) this.draft = this.draft.slice(0, -1);
  }

  cancelPlan() {
    this.draft = null;
  }

  async sendPlan() {
    if (!this.draft || this.selected === null) return;
    const res = await commands.orderRoute(this.selected, this.draft);
    this.error = res.status === "error" ? res.error : null;
    if (res.status === "ok") this.draft = null;
  }

  async orderSpeedKnots(kn: number) {
    if (this.selected === null || !Number.isFinite(kn) || kn < 0) {
      this.error = "Speed must be a number ≥ 0";
      return;
    }
    const res = await commands.orderSpeed(this.selected, kn * MPS_PER_KNOT);
    this.error = res.status === "error" ? res.error : null;
  }
}

export const sim = new SimSession();
