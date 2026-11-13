// Recognisable figures for units and munitions on the map (docs/behaviour-specs/symbology.md):
// ships, submarines, aircraft and munitions as top-down plan views drawn nose up and rotated to
// the heading on the map; land sites as simple upright icons. Each figure is filled
// in its affiliation colour and carries a small affiliation badge (the APP-6 frame shape), so
// affiliation still reads without colour. Original drawings from simple shapes; nothing is
// copied from any product.

import type { Domain } from "./bindings";
import { NAMED_FIGURE, NAMED_MUNITION } from "./named-classes";
import { AFFILIATION_COLOUR, frameSvg, type Affiliation } from "./symbols";

/** Ship and submarine plan views (64 × 64 canvas, bow up). */
type ShipKind =
  | "carrier"
  | "amphib"
  | "cruiser"
  | "destroyer"
  | "frigate"
  | "corvette"
  | "fast_attack"
  | "patrol"
  | "mcm"
  | "landing"
  | "oiler"
  | "stores"
  | "tug"
  | "survey"
  | "container"
  | "bulk"
  | "tanker"
  | "roro"
  | "trawler"
  | "supply"
  | "ship";
type SubKind = "sub_nuclear" | "sub_diesel" | "sub_midget" | "sub";
/** Top-down figures (64 × 64 canvas, nose up), rotated to the heading. */
type AirKind =
  "fighter" | "bomber" | "mpa" | "aew" | "air_tanker" | "helicopter" | "uav" | "aircraft";
/** Upright icons (64 × 64), not rotated. */
type LandKind = "missile_battery" | "sam_battery" | "radar_site" | "airbase" | "port" | "land";

export type FigureKind = ShipKind | SubKind | AirKind | LandKind;

const CLASS_FIGURE: Record<string, FigureKind> = {
  "GEN-CV": "carrier",
  "GEN-LHD": "amphib",
  "GEN-CG": "cruiser",
  "GEN-DDG": "destroyer",
  "GEN-FFG": "frigate",
  "GEN-FFG-ASW": "frigate",
  "GEN-FFL": "corvette",
  "GEN-PGG": "fast_attack",
  "GEN-OPV": "patrol",
  "GEN-MCMV": "mcm",
  "GEN-LST": "landing",
  "GEN-AOR": "oiler",
  "GEN-AKE": "stores",
  "GEN-ATF": "tug",
  "GEN-AGS": "survey",
  "GEN-AGI": "survey",
  "GEN-MV-CONTAINER": "container",
  "GEN-MV-BULK": "bulk",
  "GEN-MV-TANKER": "tanker",
  "GEN-MV-RORO": "roro",
  "GEN-MV-TRAWLER": "trawler",
  "GEN-MV-OSV": "supply",
  "GEN-SSN": "sub_nuclear",
  "GEN-SSGN": "sub_nuclear",
  "GEN-SSK": "sub_diesel",
  "GEN-SSK-AIP": "sub_diesel",
  "GEN-SSM": "sub_midget",
  "GEN-MPA-TP": "mpa",
  "GEN-MPA-JET": "mpa",
  "GEN-AEW": "aew",
  "GEN-TANKER": "air_tanker",
  "GEN-FTR-MR": "fighter",
  "GEN-FTR-AS": "fighter",
  "GEN-FTR-CV": "fighter",
  "GEN-FTR-LIGHT": "fighter",
  "GEN-BMR": "bomber",
  "GEN-HELO-ASW": "helicopter",
  "GEN-HELO-UTIL": "helicopter",
  "GEN-HELO-HEAVY": "helicopter",
  "GEN-HELO-LIGHT": "helicopter",
  "GEN-UAV-MALE": "uav",
  "GEN-UAV-VTOL": "helicopter",
  "GEN-SHORE-ASHM": "missile_battery",
  "GEN-SHORE-SAM-LR": "sam_battery",
  "GEN-SHORE-SAM-MR": "sam_battery",
  "GEN-SITE-RADAR": "radar_site",
  "GEN-AIRBASE-MAIN": "airbase",
  "GEN-AIRFIELD-FWD": "airbase",
  "GEN-PORT-NAVAL": "port",
  "GEN-PORT-COMMERCIAL": "port",
};

const CATEGORY_FIGURE: Record<string, FigureKind> = {
  surface_combatant: "ship",
  auxiliary: "oiler",
  merchant: "bulk",
  submarine: "sub",
  fighter: "fighter",
  helicopter: "helicopter",
  mpa: "mpa",
  aew: "aew",
  tanker: "air_tanker",
  uav: "uav",
  shore_battery: "missile_battery",
  sensor_site: "radar_site",
  airbase: "airbase",
  port: "port",
};

const DOMAIN_FIGURE: Record<Domain, FigureKind> = {
  surface: "ship",
  subsurface: "sub",
  air: "aircraft",
  land: "land",
};

/** The figure for what the viewer knows: the class, else the category, else the domain. */
export function figureKind(
  platform?: string | null,
  category?: string | null,
  domain?: Domain | null,
): FigureKind | null {
  return (
    (platform ? (CLASS_FIGURE[platform] ?? NAMED_FIGURE[platform]) : undefined) ??
    (category ? CATEGORY_FIGURE[category] : undefined) ??
    (domain ? DOMAIN_FIGURE[domain] : undefined) ??
    null
  );
}

const AIR_KINDS = new Set<string>([
  "fighter",
  "bomber",
  "mpa",
  "aew",
  "air_tanker",
  "helicopter",
  "uav",
  "aircraft",
]);

const SEA_KINDS = new Set<string>([
  "carrier",
  "amphib",
  "cruiser",
  "destroyer",
  "frigate",
  "corvette",
  "fast_attack",
  "patrol",
  "mcm",
  "landing",
  "oiler",
  "stores",
  "tug",
  "survey",
  "container",
  "bulk",
  "tanker",
  "roro",
  "trawler",
  "supply",
  "ship",
  "sub_nuclear",
  "sub_diesel",
  "sub_midget",
  "sub",
]);

/** How a figure is drawn on the map: top-down (rotated to the heading) or an upright icon. */
export function figureShape(kind: FigureKind): "topdown" | "icon" {
  return SEA_KINDS.has(kind) || AIR_KINDS.has(kind) ? "topdown" : "icon";
}

/**
 * Nominal size of a figure's 64 px canvas on the ground, metres. Exaggerated far beyond real
 * lengths so a figure reads at planning scale; the figure then shrinks as the map zooms out and
 * grows as it zooms in, within FIGURE_MIN_PX–FIGURE_MAX_PX. Ships keep their relative sizes,
 * because each hull is drawn at its own length on the shared canvas.
 */
export function figureSizeM(kind: FigureKind): number {
  return SEA_KINDS.has(kind) ? 9000 : 7000;
}
export const FIGURE_MIN_PX = 18;
export const FIGURE_MAX_PX = 72;

// ---------------------------------------------------------------------------------------------
// Plan views: a parametric hull on a 64 × 64 canvas, bow up, centred.

/** Deck features, positions as fractions of the hull from bow (0) to stern (1). */
type Feat =
  | ["box", number, number, number] // from, to, width as a fraction of the beam
  | ["dot", number, number] // at, radius px (a gun mount or crane)
  | ["pad", number, number] // helicopter deck, from, to
  | ["hatches", number, number, number] // from, to, count
  | ["line", number, number]; // from, to (a centreline pipe)

interface Plan {
  /** Hull length on the canvas, px (at most 62). */
  len: number;
  /** Beam, px. */
  beam: number;
  feats: Feat[];
}

const DECK = `fill="#0b1b2b" fill-opacity="0.32" stroke="none"`;
const MARK = `fill="none" stroke="#0b1b2b" stroke-opacity="0.5" stroke-width="0.8"`;

/** Beams are drawn wider than true (as in recognition sketches), so a hull reads at map size. */
const BEAM_X = 1.6;

function planSvg(plan: Plan): string {
  const p = { ...plan, beam: plan.beam * BEAM_X };
  const y0 = 32 - p.len / 2;
  const y1 = 32 + p.len / 2;
  const b = p.beam / 2;
  const at = (f: number) => y0 + p.len * f;
  const hull = `<path d="M32 ${y0} Q${32 + b} ${at(0.12)} ${32 + b} ${at(0.3)} L${32 + b * 0.92} ${y1} L${32 - b * 0.92} ${y1} L${32 - b} ${at(0.3)} Q${32 - b} ${at(0.12)} 32 ${y0} Z"/>`;
  const feats = p.feats
    .map((f) => {
      switch (f[0]) {
        case "box": {
          const w = p.beam * f[3];
          return `<rect x="${32 - w / 2}" y="${at(f[1])}" width="${w}" height="${at(f[2]) - at(f[1])}" ${DECK}/>`;
        }
        case "dot":
          return `<circle cx="32" cy="${at(f[1])}" r="${f[2]}" ${DECK}/>`;
        case "pad": {
          const w = p.beam * 0.8;
          const y = at(f[1]);
          const h = at(f[2]) - y;
          return `<rect x="${32 - w / 2}" y="${y}" width="${w}" height="${h}" ${MARK}/><path d="M${32 - w / 5} ${y + h / 2} H${32 + w / 5}" ${MARK}/>`;
        }
        case "hatches": {
          const count = f[3];
          const step = (at(f[2]) - at(f[1])) / count;
          const w = p.beam * 0.6;
          return Array.from({ length: count }, (_, k) => {
            const y = at(f[1]) + k * step + step * 0.15;
            return `<rect x="${32 - w / 2}" y="${y}" width="${w}" height="${step * 0.7}" ${DECK}/>`;
          }).join("");
        }
        case "line":
          return `<path d="M32 ${at(f[1])} V${at(f[2])}" ${MARK}/>`;
      }
    })
    .join("");
  return hull + feats;
}

/** A carrier or amphibious ship: a full-length flight deck with the island to starboard. */
function flatTopSvg(len: number, deck: number, angled: boolean): string {
  const y0 = 32 - len / 2;
  const y1 = 32 + len / 2;
  const hw = deck / 2;
  const flight = `<path d="M${32 - hw + 2} ${y0} H${32 + hw - 2} L${32 + hw} ${y0 + 4} V${y1} H${32 - hw} V${y0 + 4} Z"/>`;
  const island = `<rect x="${32 + hw - 4}" y="${y0 + len * 0.4}" width="3" height="${len * 0.16}" ${DECK}/>`;
  const marks = angled
    ? `<path d="M${32 - hw + 2} ${y0 + len * 0.15} L${32 + 1} ${y1 - 2}" ${MARK}/>`
    : `<path d="M32 ${y0 + 4} V${y1 - 3}" ${MARK} stroke-dasharray="3 2"/>`;
  return flight + island + marks;
}

/** A submarine: a long rounded hull, the sail forward of midships, tail fins across the stern. */
function subPlanSvg(len: number, trueBeam: number): string {
  const y0 = 32 - len / 2;
  const y1 = 32 + len / 2;
  const b = (trueBeam * BEAM_X) / 2;
  const hull = `<path d="M32 ${y0} C${32 + b} ${y0} ${32 + b} ${y0 + len * 0.15} ${32 + b} ${y0 + len * 0.25} L${32 + b} ${y0 + len * 0.7} Q${32 + b * 0.6} ${y1} 32 ${y1} Q${32 - b * 0.6} ${y1} ${32 - b} ${y0 + len * 0.7} L${32 - b} ${y0 + len * 0.25} C${32 - b} ${y0 + len * 0.15} ${32 - b} ${y0} 32 ${y0} Z"/>`;
  const sail = `<rect x="${32 - b * 0.45}" y="${y0 + len * 0.22}" width="${b * 0.9}" height="${len * 0.14}" rx="1" ${DECK}/>`;
  const fins = `<path d="M${32 - b - 2.5} ${y1 - len * 0.06} H${32 + b + 2.5}" stroke-width="1.6"/>`;
  return hull + sail + fins;
}

const PLANS: Record<Exclude<ShipKind, "carrier" | "amphib">, Plan> = {
  cruiser: {
    len: 58,
    beam: 9,
    feats: [
      ["dot", 0.15, 2.4],
      ["box", 0.21, 0.29, 0.6],
      ["box", 0.34, 0.6, 0.7],
      ["box", 0.62, 0.7, 0.55],
      ["pad", 0.82, 0.98],
    ],
  },
  destroyer: {
    len: 54,
    beam: 8.5,
    feats: [
      ["dot", 0.17, 2.2],
      ["box", 0.23, 0.3, 0.55],
      ["box", 0.34, 0.58, 0.7],
      ["pad", 0.8, 0.98],
    ],
  },
  frigate: {
    len: 48,
    beam: 8,
    feats: [
      ["dot", 0.18, 2.1],
      ["box", 0.3, 0.55, 0.7],
      ["box", 0.55, 0.68, 0.75],
      ["pad", 0.7, 0.98],
    ],
  },
  corvette: {
    len: 40,
    beam: 7,
    feats: [
      ["dot", 0.2, 1.9],
      ["box", 0.35, 0.62, 0.7],
      ["box", 0.7, 0.8, 0.5],
    ],
  },
  fast_attack: {
    len: 30,
    beam: 6,
    feats: [
      ["dot", 0.2, 1.5],
      ["box", 0.38, 0.6, 0.7],
      ["box", 0.66, 0.86, 0.55],
    ],
  },
  patrol: {
    len: 42,
    beam: 8,
    feats: [
      ["dot", 0.18, 1.7],
      ["box", 0.3, 0.55, 0.75],
      ["pad", 0.74, 0.97],
    ],
  },
  mcm: {
    len: 34,
    beam: 7.5,
    feats: [
      ["box", 0.3, 0.6, 0.75],
      ["box", 0.75, 0.95, 0.6],
    ],
  },
  landing: {
    len: 54,
    beam: 11,
    feats: [
      ["hatches", 0.08, 0.66, 3],
      ["box", 0.7, 0.92, 0.85],
    ],
  },
  oiler: {
    len: 60,
    beam: 11,
    feats: [
      ["box", 0.08, 0.18, 0.8],
      ["box", 0.34, 0.37, 1],
      ["box", 0.5, 0.53, 1],
      ["line", 0.2, 0.75],
      ["box", 0.78, 0.95, 0.85],
    ],
  },
  stores: {
    len: 58,
    beam: 11,
    feats: [
      ["dot", 0.28, 1.8],
      ["dot", 0.5, 1.8],
      ["hatches", 0.12, 0.7, 3],
      ["box", 0.78, 0.95, 0.85],
    ],
  },
  tug: { len: 24, beam: 8, feats: [["box", 0.2, 0.55, 0.75]] },
  survey: {
    len: 40,
    beam: 8,
    feats: [
      ["box", 0.2, 0.5, 0.75],
      ["box", 0.75, 0.95, 0.6],
    ],
  },
  container: {
    len: 62,
    beam: 12,
    feats: [
      ["hatches", 0.12, 0.78, 6],
      ["box", 0.8, 0.93, 0.9],
    ],
  },
  bulk: {
    len: 60,
    beam: 11,
    feats: [
      ["hatches", 0.14, 0.74, 5],
      ["box", 0.8, 0.95, 0.85],
    ],
  },
  tanker: {
    len: 62,
    beam: 12,
    feats: [
      ["line", 0.08, 0.78],
      ["box", 0.82, 0.96, 0.85],
    ],
  },
  roro: {
    len: 56,
    beam: 11,
    feats: [
      ["box", 0.06, 0.94, 0.88],
      ["box", 0.12, 0.3, 0.6],
    ],
  },
  trawler: {
    len: 26,
    beam: 7,
    feats: [
      ["box", 0.2, 0.45, 0.7],
      ["box", 0.75, 0.95, 0.6],
    ],
  },
  supply: { len: 40, beam: 9, feats: [["box", 0.06, 0.3, 0.85]] },
  ship: { len: 46, beam: 8, feats: [["box", 0.35, 0.6, 0.7]] },
};

function seaSvg(kind: ShipKind | SubKind): string {
  switch (kind) {
    case "carrier":
      return flatTopSvg(60, 20, true);
    case "amphib":
      return flatTopSvg(58, 15, false);
    case "sub_nuclear":
      return subPlanSvg(60, 7);
    case "sub_midget":
      return subPlanSvg(26, 4);
    case "sub_diesel":
    case "sub":
      return subPlanSvg(46, 6.5);
    default:
      return planSvg(PLANS[kind]);
  }
}

// ---------------------------------------------------------------------------------------------
// Top-down outlines, 64 × 64, nose up.

const TOPDOWN: Record<AirKind, string> = {
  fighter: `<path d="M32 4 L35 18 L35 26 L56 44 L56 48 L35 42 L34 52 L42 58 L42 60 L32 57 L22 60 L22 58 L30 52 L29 42 L8 48 L8 44 L29 26 L29 18 Z"/>`,
  bomber: `<path d="M32 3 L35 14 L35 24 L60 40 L60 44 L35 36 L34 52 L44 58 L44 61 L32 58 L20 61 L20 58 L30 52 L29 36 L4 44 L4 40 L29 24 L29 14 Z"/>`,
  mpa: `<path d="M30 4 H34 L35 22 H60 V27 H35 L34 50 L44 52 V56 H20 V52 L30 50 L29 27 H4 V22 H29 Z"/><rect x="12" y="19" width="4" height="9"/><rect x="48" y="19" width="4" height="9"/>`,
  aew: `<path d="M30 4 H34 L35 22 H60 V27 H35 L34 50 L44 52 V56 H20 V52 L30 50 L29 27 H4 V22 H29 Z"/><circle cx="32" cy="38" r="9" fill-opacity="0.75"/>`,
  air_tanker: `<path d="M30 4 H34 L35 24 L60 36 V40 L35 34 L34 50 L43 54 V57 H21 V54 L30 50 L29 34 L4 40 V36 L29 24 Z"/><path d="M32 57 V63" fill="none"/>`,
  helicopter: `<circle cx="32" cy="24" r="20" fill="none" stroke-dasharray="4 3"/><path d="M27 12 H37 L38 30 L34 36 L33 56 H38 V59 H26 V56 H31 L30 36 L26 30 Z"/>`,
  uav: `<path d="M31 8 H33 L34 28 H62 V31 H34 L33 50 L42 58 V60 L32 56 L22 60 V58 L31 50 L30 31 H2 V28 H30 Z"/>`,
  aircraft: `<path d="M30 6 H34 L35 24 L58 34 V38 L35 33 L34 50 L42 54 V57 H22 V54 L30 50 L29 33 L6 38 V34 L29 24 Z"/>`,
};

// ---------------------------------------------------------------------------------------------
// Land icons, 64 × 64, upright.

const LAND: Record<LandKind, string> = {
  missile_battery: `<rect x="8" y="40" width="42" height="10"/><circle cx="16" cy="53" r="4"/><circle cx="40" cy="53" r="4"/><path d="M20 40 L50 18 L54 23 L26 42 Z"/><rect x="50" y="34" width="8" height="16"/>`,
  sam_battery: `<rect x="8" y="42" width="44" height="9"/><circle cx="16" cy="54" r="4"/><circle cx="42" cy="54" r="4"/><rect x="18" y="20" width="7" height="22"/><rect x="27" y="20" width="7" height="22"/><rect x="36" y="20" width="7" height="22"/>`,
  radar_site: `<path d="M30 58 H34 L33 34 H31 Z"/><path d="M14 18 Q32 42 50 18 Z"/><path d="M32 30 V20" fill="none"/>`,
  airbase: `<rect x="10" y="26" width="44" height="12" transform="rotate(-20 32 32)"/><path d="M14 46 H50 V54 H14 Z"/>`,
  port: `<path d="M6 46 H58 V52 H6 Z"/><path d="M40 46 V14 H44 V46 Z"/><path d="M44 16 H58 L44 26 Z"/><path d="M52 18 V36" fill="none"/>`,
  land: `<rect x="12" y="24" width="40" height="22"/>`,
};

// ---------------------------------------------------------------------------------------------
// Munitions, 64 × 64, nose up.

export type MunitionKind =
  | "anti_ship_missile"
  | "sam"
  | "cruise_missile"
  | "air_to_air"
  | "torpedo"
  | "asw_rocket"
  | "bomb"
  | "shell"
  | "depth_charge"
  | "decoy"
  | "mine";

/** The munition figure for a weapon id from the database. */
export function munitionKind(weapon: string): MunitionKind {
  // Named weapons (the regional database) by their database kind.
  const named = NAMED_MUNITION[weapon];
  if (named) return named;
  if (weapon.startsWith("WPN-ASM")) return "anti_ship_missile";
  if (weapon.startsWith("WPN-SAM")) return "sam";
  if (weapon.startsWith("WPN-LACM")) return "cruise_missile";
  if (weapon.startsWith("WPN-AAM")) return "air_to_air";
  if (weapon.startsWith("WPN-TORP")) return "torpedo";
  if (weapon.startsWith("WPN-ASW-ROCKET")) return "asw_rocket";
  if (weapon.startsWith("WPN-BOMB")) return "bomb";
  if (weapon.startsWith("WPN-DEPTH")) return "depth_charge";
  if (weapon.startsWith("WPN-DECOY")) return "decoy";
  if (weapon.startsWith("WPN-MINE")) return "mine";
  // Guns and CIWS, and anything not listed: a shell.
  return "shell";
}

/** Plain words for a munition, for notices. */
export const MUNITION_LABEL: Record<MunitionKind, string> = {
  anti_ship_missile: "anti-ship missile",
  sam: "surface-to-air missile",
  cruise_missile: "cruise missile",
  air_to_air: "air-to-air missile",
  torpedo: "torpedo",
  asw_rocket: "ASW rocket",
  bomb: "bomb",
  shell: "gunfire",
  depth_charge: "depth charge",
  decoy: "decoy",
  mine: "mine",
};

const MUNITION: Record<MunitionKind, string> = {
  anti_ship_missile: `<path d="M32 4 L36 14 V48 L44 58 V61 H20 V58 L28 48 V14 Z"/><path d="M28 28 L22 34 V37 L28 33 Z M36 28 L42 34 V37 L36 33 Z"/>`,
  sam: `<path d="M32 3 L35 12 V52 L41 60 V62 H23 V60 L29 52 V12 Z"/><path d="M29 24 L24 30 V32 L29 29 Z M35 24 L40 30 V32 L35 29 Z"/>`,
  cruise_missile: `<path d="M32 5 L36 14 V50 L43 58 V61 H21 V58 L28 50 V14 Z"/><path d="M28 30 L12 36 V39 L28 35 Z M36 30 L52 36 V39 L36 35 Z"/>`,
  air_to_air: `<path d="M32 4 L34 12 V54 L39 60 V62 H25 V60 L30 54 V12 Z"/><path d="M30 20 L26 25 V27 L30 25 Z M34 20 L38 25 V27 L34 25 Z"/>`,
  torpedo: `<path d="M27 10 A5 5 0 0 1 37 10 V50 L40 56 H24 L27 50 Z"/><path d="M22 58 H42 M32 56 V62" fill="none"/>`,
  asw_rocket: `<path d="M32 10 L36 18 V46 L41 54 V56 H23 V54 L28 46 V18 Z"/>`,
  bomb: `<path d="M32 8 C40 8 40 30 38 44 L44 54 V57 H20 V54 L26 44 C24 30 24 8 32 8 Z"/>`,
  shell: `<path d="M32 16 L36 26 V46 H28 V26 Z"/>`,
  depth_charge: `<rect x="24" y="22" width="16" height="22" rx="3"/>`,
  decoy: `<circle cx="26" cy="28" r="7"/><circle cx="38" cy="26" r="6"/><circle cx="33" cy="38" r="8"/>`,
  mine: `<circle cx="32" cy="34" r="12"/><path d="M32 18 V22 M32 46 V50 M16 34 H20 M44 34 H48" fill="none"/>`,
};

// ---------------------------------------------------------------------------------------------

function rgb([r, g, b]: [number, number, number]) {
  return `rgb(${r},${g},${b})`;
}

const cache = new Map<string, string>();

/**
 * The affiliation badge: the APP-6 frame for the affiliation and domain, so a unit's side reads
 * by shape as well as colour. The map draws it beside the unit's label, facing the screen; it is
 * not part of the figure, which turns with the heading.
 */
export function badgeUrl(aff: Affiliation, domain: Domain): string {
  const key = `b-${aff}-${domain}`;
  const hit = cache.get(key);
  if (hit !== undefined) return hit;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><g fill="${rgb(AFFILIATION_COLOUR[aff])}" stroke="#0b1b2b" stroke-width="5">${frameSvg(aff, domain)}</g></svg>`;
  const url = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  cache.set(key, url);
  return url;
}

/** Canvas size of a figure's icon, px (every figure is drawn on 64 × 64). */
export function figureCanvas(kind: FigureKind): [number, number] {
  void kind;
  return [64, 64];
}

/**
 * An SVG data URL for a figure in its affiliation colour, nose up (the map turns it to the
 * heading). Cached by kind, affiliation and selection.
 */
export function figureUrl(kind: FigureKind, aff: Affiliation, selected: boolean): string {
  const key = `${kind}-${aff}-${selected}`;
  const hit = cache.get(key);
  if (hit !== undefined) return hit;
  const body = SEA_KINDS.has(kind)
    ? seaSvg(kind as ShipKind | SubKind)
    : AIR_KINDS.has(kind)
      ? TOPDOWN[kind as AirKind]
      : LAND[kind as LandKind];
  const stroke = selected ? "#ffffff" : "#0b1b2b";
  const sw = selected ? 2 : 1.2;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><g fill="${rgb(AFFILIATION_COLOUR[aff])}" stroke="${stroke}" stroke-width="${sw}" stroke-linejoin="round">${body}</g></svg>`;
  const url = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  cache.set(key, url);
  return url;
}

/** An SVG data URL for a munition, nose up, coloured by who fired it; a homing weapon is outlined
 * in white. */
export function munitionUrl(kind: MunitionKind, aff: Affiliation, homing: boolean): string {
  const key = `m-${kind}-${aff}-${homing}`;
  const hit = cache.get(key);
  if (hit !== undefined) return hit;
  const stroke = homing ? "#ffffff" : "#0b1b2b";
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><g fill="${rgb(AFFILIATION_COLOUR[aff])}" stroke="${stroke}" stroke-width="2" stroke-linejoin="round">${MUNITION[kind]}</g></svg>`;
  const url = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  cache.set(key, url);
  return url;
}
