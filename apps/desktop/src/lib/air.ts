// Air operations in the panels (Phase 4 slices 9–11, docs/models/air-operations.md): what a
// parked aircraft is doing, what an airborne one is doing, and the missions a seat can launch.
// The engine decides; this module only words it and builds orders.

import type {
  AirUnitView,
  GeoPoint,
  Mission_Deserialize as Mission,
  ParkedView,
  Readiness,
} from "./bindings";
import { n } from "./format";

/** The sorties a seat can launch from the Air tab. */
export type SortieKind = "patrol" | "strike" | "support" | "ferry";

export const SORTIE_KINDS: { key: SortieKind; label: string }[] = [
  { key: "patrol", label: "Patrol (CAP, ASW)" },
  { key: "strike", label: "Strike a contact" },
  { key: "support", label: "Hold a station" },
  { key: "ferry", label: "Fly to a point" },
];

export const READINESS: { key: Readiness; label: string }[] = [
  { key: "ready", label: "Ready" },
  { key: "alert15", label: "Alert 15" },
  { key: "alert60", label: "Alert 60" },
  { key: "maintenance", label: "Maintenance" },
];

const minutes = (s: number) => Math.max(1, Math.ceil(s / 60));

/** What a parked aircraft is doing, in a few words. */
export function parkedText(p: ParkedView): string {
  const wait = n(p.ready_in_s);
  if (p.damaged) return "damaged: cannot fly";
  if (p.launching) return wait > 0 ? `launching in ${minutes(wait)} min` : "waiting for the runway";
  if (p.readiness === "maintenance")
    return wait > 0 ? `maintenance, ${minutes(wait)} min` : "maintenance";
  if (wait > 0) return `turning round, ${minutes(wait)} min`;
  return READINESS.find((r) => r.key === p.readiness)?.label.toLowerCase() ?? p.readiness;
}

/** What an airborne aircraft is doing, in a few words. */
export function airborneText(a: AirUnitView): string {
  switch (a.state) {
    case "returning":
      return "returning to base";
    case "to_tanker":
      return "heading for a tanker";
    case "tanking":
      return "refuelling from a tanker";
    default:
      return a.tanker ? "on station (tanker)" : "on a sortie";
  }
}

/** Bingo as a share of the tank, for the fuel row. */
export function bingoText(a: AirUnitView): string | null {
  return a.bingo_fraction == null ? null : `bingo ${Math.round(100 * n(a.bingo_fraction))} %`;
}

/** Whether a sortie needs a point picked on the map. */
export function needsPoint(kind: SortieKind): boolean {
  return kind !== "strike";
}

/** The mission for a sortie: a point (patrol, support, ferry) or a contact (strike). */
export function sortieMission(
  kind: SortieKind,
  at: GeoPoint | null,
  contact: number | null,
  radiusM: number,
): Mission | null {
  switch (kind) {
    case "patrol":
      return at ? { kind: "patrol", centre: at, radius_m: radiusM } : null;
    case "support":
      return at ? { kind: "support", station: at, radius_m: radiusM } : null;
    case "ferry":
      return at ? { kind: "ferry", destination: at } : null;
    case "strike":
      return contact == null ? null : { kind: "strike", contact };
  }
}
