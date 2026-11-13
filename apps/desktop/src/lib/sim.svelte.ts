// UI-side session state. It holds the latest frame from the engine plus purely local UI state
// (selection, draft route). Every change to the simulation goes through `commands`.

import {
  type WeatherZone_Serialize as WeatherZone,
  type SoundProfile_Serialize as SoundProfile,
  type SonobuoyView,
  type Lane_Deserialize as Lane,
  type Outcome,
  type ActionGroup,
  type AarTable,
  type Facility,
  type Zone,
  type AarView,
  type Adjudication,
  type AdjudicationRecord,
  commands,
  events,
  type BearingView,
  type ClockStatus,
  type CommsView,
  type ContactView,
  type Doctrine,
  type Environment,
  type EventStatus,
  type Facilitation,
  type InjectStatus,
  type FrameEvent,
  type GeoPoint,
  type Message,
  type Minefield,
  type Mission,
  type PostView,
  type WeaponTrackView,
  type ZoneCoverage,
  type SessionInfo,
  type TimeCompression,
  type UnitView,
  type WeaponView,
} from "./bindings";
import { MPS_PER_KNOT } from "./format";
import { download, fileStem } from "./files";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

/** Radius of a patrol ordered from the unit panel, metres. */
export const PATROL_RADIUS_M = 30_000;

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
  /** Sea state and weather, from the latest frame. */
  environment = $state<Environment | null>(null);
  /** Weather zones over the scenario-wide weather (every seat sees them). */
  weatherZones = $state<WeatherZone[]>([]);
  /** Sound-velocity profiles in force (every seat sees them). */
  soundProfiles = $state<SoundProfile[]>([]);
  /** The seat's laid sonobuoys. */
  sonobuoys = $state<SonobuoyView[]>([]);
  /** The seat's recent ESM bearing lines. */
  bearings = $state<BearingView[]>([]);
  /** The seat's missiles and torpedoes in flight. */
  weapons = $state<WeaponView[]>([]);
  /** The seat's minefields. */
  minefields = $state<Minefield[]>([]);
  /** White cell: the most recent overrides. */
  adjudications = $state<AdjudicationRecord[]>([]);
  /** The seat's message log, oldest first. */
  messages = $state<Message[]>([]);
  /** White cell: the inject library with send counts. */
  injects = $state<InjectStatus[]>([]);
  /** White cell: every scenario event's state. */
  scenarioEvents = $state<EventStatus[]>([]);
  /** Comms link of each of the seat's units (sides with command posts). */
  comms = $state<CommsView[]>([]);
  /** The seat's command posts and whether each is up. */
  posts = $state<PostView[]>([]);
  /** The seat's tracks on other sides' missiles and torpedoes. */
  weaponTracks = $state<WeaponTrackView[]>([]);
  /** The seat's surface action groups. */
  groups = $state<ActionGroup[]>([]);
  /** Weapon outcomes of the last minute this seat may know (hit, miss, shot down). */
  outcomes = $state<Outcome[]>([]);

  /** The action group a unit is in, if any. */
  groupOf(unit: number): ActionGroup | undefined {
    return this.groups.find((g) => g.leader === unit || g.members.some((m) => m.unit === unit));
  }

  async detach(unit: number) {
    const res = await commands.orderDetach(unit);
    this.error = res.status === "error" ? res.error : null;
  }

  /** Joins a unit to a group, 3 km astern of the leader. */
  async join(unit: number, group: string) {
    const res = await commands.orderJoin(unit, group, 180, 3_000);
    this.error = res.status === "error" ? res.error : null;
  }
  /** Sense zones whose coverage is drawn, and their latest coverage. */
  coverageShown = $state<number[]>([]);
  coverage = $state<ZoneCoverage[]>([]);

  /** Shows or hides one sense zone's coverage. */
  async toggleCoverage(zone: number) {
    this.coverageShown = this.coverageShown.includes(zone)
      ? this.coverageShown.filter((z) => z !== zone)
      : [...this.coverageShown, zone];
    await this.refreshCoverage();
  }

  /** Asks the engine for the shown zones' coverage now. */
  async refreshCoverage() {
    const out: ZoneCoverage[] = [];
    for (const zone of this.coverageShown) {
      const res = await commands.zoneCoverage(zone);
      if (res.status === "ok" && res.data) out.push(res.data);
      else if (res.status === "error") this.error = res.error;
    }
    this.coverage = out;
  }
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
    // Each window listens to its own frames: a seat window sees only its seat's picture.
    // Listen first: asking for the session then makes the next frame a full picture, which
    // would be lost if it came before the listener (a slow cold start).
    await events.frameEvent(getCurrentWebviewWindow()).listen((e) => this.apply(e.payload));
    await this.resync();
  }

  /** Fetches the session; the backend answers by sending this window a full picture next. */
  private async resync() {
    const res = await commands.getSession();
    if (res.status === "error") throw new Error(res.error);
    this.session = res.data;
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
    this.environment = u.environment;
    this.weatherZones = (u.weather_zones ?? []) as WeatherZone[];
    this.soundProfiles = (u.sound_profiles ?? []) as SoundProfile[];
    this.sonobuoys = u.sonobuoys ?? [];
    this.bearings = u.bearings;
    this.weapons = u.weapons;
    this.minefields = u.minefields;
    this.adjudications = u.adjudications;
    this.messages = u.messages;
    this.injects = u.injects;
    this.scenarioEvents = u.scenario_events;
    this.comms = u.comms;
    this.posts = u.posts;
    this.weaponTracks = u.weapon_tracks;
    this.groups = u.action_groups;
    this.outcomes = u.outcomes ?? [];
    if (u.refusals.length > 0) this.error = u.refusals[u.refusals.length - 1] ?? null;
  }

  /** Loads a scenario file the user picked; the engine validates it and resets the session. */
  async loadScenario(file: File) {
    await this.loadScenarioText(file.name, await file.text());
  }

  /** Loads scenario JSON (from a file or the scenario editor). True if it loaded. */
  async loadScenarioText(name: string, json: string): Promise<boolean> {
    const res = await commands.loadScenario(json);
    if (res.status === "error") {
      this.error = `Could not load ${name}: ${res.error}`;
      return false;
    }
    this.session = res.data;
    this.clock = null;
    this.units = [];
    this.allContacts = [];
    this.selected = null;
    this.draft = null;
    this.error = null;
    // The new scenario's full picture may have come before this reply and been cleared above.
    await this.resync();
    return true;
  }

  /** Can this window run the clock? (The main window and white-cell seats can.) */
  get controlsTime(): boolean {
    return this.session?.controls_time ?? false;
  }

  async togglePause() {
    if (!this.controlsTime) return;
    const res = await commands.setPaused(!(this.clock?.paused ?? true));
    if (res.status === "error") this.error = res.error;
  }

  async setCompression(c: TimeCompression) {
    if (!this.controlsTime) return;
    const res = await commands.setTimeCompression(c);
    if (res.status === "error") this.error = res.error;
  }

  async step(ticks: number) {
    if (!this.controlsTime) return;
    const res = await commands.stepTicks(ticks);
    if (res.status === "error") this.error = res.error;
  }

  /** Moves this (unlocked) window to another seat. */
  async setSeat(seat: number) {
    this.cancelPlan();
    this.selected = null;
    const res = await commands.setSeat(seat);
    if (res.status === "error") {
      this.error = res.error;
      return;
    }
    this.session = res.data;
    // Drop the old seat's picture and ask for the new seat's in full.
    this.units = [];
    this.allContacts = [];
    await this.resync();
  }

  /** Adds a seat for a side, or a white-cell seat when `side` is null. */
  async addSeat(name: string, side: number | null) {
    if (!name.trim()) {
      this.error = "Give the seat a name";
      return false;
    }
    const res = await commands.addSeat(name.trim(), side);
    if (res.status === "error") {
      this.error = res.error;
      return false;
    }
    this.session = res.data;
    this.error = null;
    return true;
  }

  async removeSeat(seat: number) {
    const res = await commands.removeSeat(seat);
    if (res.status === "error") this.error = res.error;
    else this.session = res.data;
  }

  /** Opens a window locked to `seat`, e.g. on a second screen for a side's commander. */
  async openSeatWindow(seat: number) {
    const res = await commands.openSeatWindow(seat);
    this.error = res.status === "error" ? res.error : null;
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

  /** Fires `rounds` of `weapon` from the selected unit at contact `contact`. A refusal (range,
   * track quality, weapons control status…) arrives in a later frame and is shown as an error. */
  async orderEngage(contact: number, weapon: string, rounds: number) {
    if (this.selected === null) return;
    if (!Number.isInteger(rounds) || rounds < 1) {
      this.error = "Rounds must be a whole number ≥ 1";
      return;
    }
    const res = await commands.orderEngage(this.selected, contact, weapon, rounds);
    this.error = res.status === "error" ? res.error : null;
  }

  /** Patrols the selected unit around its current position (30 km radius), or clears its
   * mission with `null`. */
  async orderPatrolHere(clear = false) {
    const u = this.selectedUnit;
    if (!u) return;
    const mission: Mission | null = clear
      ? null
      : { kind: "patrol", centre: u.position, radius_m: PATROL_RADIUS_M };
    // The leader of an action group orders the group's one mission.
    const g = this.groupOf(u.id);
    const res =
      g && g.leader === u.id
        ? await commands.orderGroupMission(g.name, mission)
        : await commands.orderMission(u.id, mission);
    this.error = res.status === "error" ? res.error : null;
  }

  /** A white-cell override with its reason. The engine refuses one with no reason. */
  async adjudicate(reason: string, action: Adjudication) {
    if (!reason.trim()) {
      this.error = "Give a reason for the override";
      return;
    }
    const res = await commands.orderAdjudicate(reason.trim(), action);
    this.error = res.status === "error" ? res.error : null;
  }

  /** A white-cell facilitation action: inject, message, fire or switch an event. */
  /** White cell: changes one zone's rules (weapons control, ROE, emissions). */
  async setZoneDoctrine(zone: number, doctrine: Doctrine) {
    const res = await commands.orderZoneDoctrine(zone, doctrine);
    if (res.status === "error") this.error = res.error;
    else {
      this.session = res.data;
      this.error = null;
    }
  }

  /** White cell: replaces the planning layer's facilities and zones in play. */
  /** Sets the planning layer; `lanes` null keeps the lanes in play. */
  async setPlanning(facilities: Facility[], zones: Zone[], lanes: Lane[] | null = null) {
    const res = await commands.orderPlanning(facilities, zones, lanes);
    if (res.status === "error") this.error = res.error;
    else {
      this.session = res.data;
      this.error = null;
    }
  }

  /** A point being picked on the map for the white cell (a new zone's centre). */
  picking = $state<{ prompt: string; done: (p: GeoPoint) => void } | null>(null);

  pick(prompt: string, done: (p: GeoPoint) => void) {
    this.picking = { prompt, done };
  }

  /** A map click while picking. True if it was used. */
  deliverPick(p: GeoPoint): boolean {
    const pk = this.picking;
    if (!pk) return false;
    this.picking = null;
    pk.done(p);
    return true;
  }

  async facilitate(action: Facilitation) {
    if (action.kind === "send_message" && !action.text.trim()) {
      this.error = "Write the message first";
      return;
    }
    const res = await commands.orderFacilitate(action);
    this.error = res.status === "error" ? res.error : null;
  }

  /** Is the session reviewing a replay (from the latest frame)? */
  get replaying(): boolean {
    return this.clock?.replay != null;
  }

  /** Replays the live exercise from the start; the live run is paused and kept. */
  async review() {
    this.cancelPlan();
    const res = await commands.reviewExercise();
    this.error = res.status === "error" ? res.error : null;
  }

  /** Leaves the replay and returns to the live exercise. */
  async closeReview() {
    const res = await commands.closeReview();
    this.error = res.status === "error" ? res.error : null;
  }

  /** Moves the replay scrubber to `tick`. */
  async seek(tick: number) {
    const res = await commands.seekReplay(Math.max(0, Math.round(tick)));
    if (res.status === "error") this.error = res.error;
  }

  /** Saves the exercise record (the replay under review, or the live run so far). */
  async saveRecord() {
    const res = await commands.saveRecord();
    if (res.status === "error") {
      this.error = res.error;
      return;
    }
    const name = fileStem(this.session?.scenario_name ?? "exercise");
    download(`${name}.nwsrec`, new Uint8Array(res.data), "application/octet-stream");
  }

  /** Opens a saved exercise record for review: its scenario replaces the session. */
  async openRecord(file: File) {
    const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
    const res = await commands.openRecord(bytes);
    if (res.status === "error") {
      this.error = `Could not open ${file.name}: ${res.error}`;
      return;
    }
    this.session = res.data;
    this.units = [];
    this.allContacts = [];
    this.selected = null;
    this.draft = null;
    this.error = null;
    await this.resync();
  }

  /** The after-action review tables (of the replay, or of the live run so far). */
  async aar(): Promise<AarView | null> {
    const res = await commands.getAar();
    if (res.status === "error") {
      this.error = res.error;
      return null;
    }
    return res.data;
  }

  /** Saves one AAR table as a CSV file. */
  async saveCsv(table: AarTable) {
    const res = await commands.aarCsv(table);
    if (res.status === "error") {
      this.error = res.error;
      return;
    }
    const name = fileStem(this.session?.scenario_name ?? "exercise");
    download(`${name}-${table}.csv`, res.data, "text/csv");
  }

  /** Changes doctrine set on the selected unit; fields given as null go back to inheriting. */
  async orderDoctrine(change: Partial<Doctrine>) {
    const u = this.selectedUnit;
    if (!u) return;
    const res = await commands.orderDoctrine(u.id, { ...u.doctrine_set, ...change });
    this.error = res.status === "error" ? res.error : null;
  }
}

export const sim = new SimSession();
