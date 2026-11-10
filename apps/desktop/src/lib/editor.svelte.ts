// Scenario editor state (docs/models/scenario-editor.md): the draft, its check results, the
// database pick lists, and map picking. The engine checks every draft (`editor_check`); saving
// and playing are refused while it reports errors.

import {
  commands,
  type DbPlatformRow,
  type DbWeaponRow,
  type GeoPoint,
  type ScenarioIssue,
} from "./bindings";
import { download, fileStem } from "./files";
import { n } from "./format";
import { blankScenario, cloneDraft, toJson, type DraftScenario } from "./scenario-edit";
import { sim } from "./sim.svelte";

/** A map pick in progress: the next click (or every click, for a route) goes to `done`. */
export interface Pick {
  prompt: string;
  /** Keep picking until `finish` (a route); otherwise one click ends it. */
  many: boolean;
  done: (p: GeoPoint) => void;
}

class ScenarioEditor {
  open = $state(false);
  draft = $state<DraftScenario>(blankScenario());
  issues = $state<ScenarioIssue[]>([]);
  /** The issues describe the draft as it is now. */
  checked = $state(false);
  busy = $state(false);
  error = $state<string | null>(null);
  selectedUnit = $state<number | null>(null);
  picking = $state<Pick | null>(null);
  platforms = $state<DbPlatformRow[]>([]);
  weapons = $state<DbWeaponRow[]>([]);

  errors = $derived(this.issues.filter((i) => i.severity === "error"));
  warnings = $derived(this.issues.filter((i) => i.severity === "warning"));

  /** Opens the editor on the scenario now loaded, a blank one, or a file. */
  async start(from: "current" | "blank" | File) {
    this.error = null;
    if (from === "blank") {
      this.draft = blankScenario();
    } else if (from === "current") {
      this.draft = cloneDraft((await commands.editorCurrent()) as unknown as DraftScenario);
    } else {
      try {
        this.draft = JSON.parse(await from.text()) as DraftScenario;
      } catch (e) {
        this.error = `${from.name} is not JSON: ${String(e)}`;
        return;
      }
    }
    this.selectedUnit = this.draft.units[0]?.id ?? null;
    this.open = true;
    await this.loadLists();
    await this.check();
  }

  async loadLists() {
    if (this.platforms.length === 0) {
      const p = await commands.dbSearch("");
      if (p.status === "ok") this.platforms = p.data;
    }
    if (this.weapons.length === 0) {
      const w = await commands.dbWeapons();
      if (w.status === "ok") this.weapons = w.data;
    }
  }

  close() {
    this.picking = null;
    this.open = false;
  }

  /** Call after every change to the draft: the old check no longer applies. */
  changed() {
    this.checked = false;
  }

  /** Asks the engine to check the draft. True when there are no errors. */
  async check(): Promise<boolean> {
    this.busy = true;
    try {
      const res = await commands.editorCheck(toJson(this.draft));
      if (res.status === "error") {
        this.error = res.error;
        return false;
      }
      this.issues = res.data.issues;
      this.checked = true;
      this.error = null;
      return this.errors.length === 0;
    } finally {
      this.busy = false;
    }
  }

  /** Saves the draft as a scenario file, if it has no errors. */
  async save() {
    if (!(await this.check())) {
      this.error = "Fix the errors before saving.";
      return;
    }
    download(`${fileStem(this.draft.name)}.json`, toJson(this.draft), "application/json");
  }

  /** Loads the draft into the session (replacing the running exercise), if it has no errors. */
  async play() {
    if (!(await this.check())) {
      this.error = "Fix the errors before playing.";
      return;
    }
    if (await sim.loadScenarioText(this.draft.name, toJson(this.draft))) this.close();
  }

  /** Starts a map pick. */
  pick(prompt: string, done: (p: GeoPoint) => void, many = false) {
    this.picking = { prompt, done, many };
  }

  /** A map click while picking. Returns true if the click was used. */
  deliver(p: GeoPoint): boolean {
    const pk = this.picking;
    if (!pk) return false;
    pk.done({ lat_deg: round(n(p.lat_deg)), lon_deg: round(n(p.lon_deg)) });
    if (!pk.many) this.picking = null;
    this.changed();
    return true;
  }

  finishPick() {
    this.picking = null;
  }
}

/** Five decimal places of a degree is about a metre: enough, and keeps files readable. */
function round(x: number): number {
  return Math.round(x * 1e5) / 1e5;
}

export const editor = new ScenarioEditor();
