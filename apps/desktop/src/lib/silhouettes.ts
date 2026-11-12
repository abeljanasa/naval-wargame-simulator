// Recognisable figures for units and munitions on the map (docs/behaviour-specs/symbology.md):
// ships and submarines as side profiles (bow toward the heading), aircraft and munitions as
// top-down outlines (rotated to the heading), land sites as simple icons. Each figure is filled
// in its affiliation colour and carries a small affiliation badge (the APP-6 frame shape), so
// affiliation still reads without colour. Original drawings from simple shapes; nothing is
// copied from any product.

import type { Domain } from "./bindings";
import { AFFILIATION_COLOUR, frameSvg, type Affiliation } from "./symbols";

/** Side-profile figures (96 × 48 canvas, bow to the right). */
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
    (platform ? CLASS_FIGURE[platform] : undefined) ??
    (category ? CATEGORY_FIGURE[category] : undefined) ??
    (domain ? DOMAIN_FIGURE[domain] : undefined) ??
    null
  );
}

const PROFILE_KINDS = new Set<string>([
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

/** How a figure is drawn on the map. */
export function figureShape(kind: FigureKind): "profile" | "topdown" | "icon" {
  if (PROFILE_KINDS.has(kind)) return "profile";
  if (AIR_KINDS.has(kind)) return "topdown";
  return "icon";
}

// ---------------------------------------------------------------------------------------------
// Side profiles: a parametric ship on a 96 × 48 canvas, waterline at y = 34, bow to the right.

interface Profile {
  /** Hull length, px (at most 88). */
  len: number;
  /** Hull height above the waterline, px. */
  fb: number;
  /** Bow rake, px. */
  rake?: number;
  /** Superstructure blocks: [x from the stern, width, height]. */
  blocks?: [number, number, number][];
  /** Masts: [x from the stern, height above the deck]. */
  masts?: [number, number][];
  /** Gun turrets: x from the stern. */
  guns?: number[];
  /** Small boxes on the deck (containers, hatches): [x, width, height]. */
  cargo?: [number, number, number][];
  /** A flight deck overhanging the hull (carriers, amphibious ships). */
  flat?: boolean;
}

const WL = 34;

function profileSvg(p: Profile): string {
  const x0 = (96 - p.len) / 2;
  const x1 = x0 + p.len;
  const top = WL - p.fb;
  const rake = p.rake ?? Math.min(10, p.len * 0.12);
  const hull = `<path d="M${x0} ${top} L${x1} ${top} L${x1 - rake} ${WL + 3} L${x0 + 2} ${WL + 3} L${x0} ${WL} Z"/>`;
  const deck = p.flat ? `<rect x="${x0 - 2}" y="${top - 3}" width="${p.len + 4}" height="3"/>` : "";
  const boxes = (list: [number, number, number][] | undefined) =>
    (list ?? [])
      .map(([x, w, h]) => `<rect x="${x0 + x}" y="${top - h}" width="${w}" height="${h}"/>`)
      .join("");
  const masts = (p.masts ?? [])
    .map(([x, h]) => {
      const mx = x0 + x;
      return `<path d="M${mx} ${top} V${top - h} M${mx - 3} ${top - h + 3} H${mx + 3}" fill="none"/>`;
    })
    .join("");
  const guns = (p.guns ?? [])
    .map((x) => {
      const gx = x0 + x;
      return `<rect x="${gx - 2.5}" y="${top - 3}" width="5" height="3"/><path d="M${gx + 2} ${top - 2} H${gx + 7}" fill="none"/>`;
    })
    .join("");
  return deck + hull + boxes(p.blocks) + boxes(p.cargo) + masts + guns;
}

/** A submarine: a long rounded hull at the waterline, a sail and a rudder. */
function subSvg(len: number, h: number, sailAt: number): string {
  const x0 = (96 - len) / 2;
  const x1 = x0 + len;
  const y = WL - h / 2;
  const hull = `<path d="M${x0 + 4} ${y} H${x1 - h / 2} A${h / 2} ${h / 2} 0 0 1 ${x1 - h / 2} ${y + h} H${x0 + 4} L${x0} ${y + h / 2} Z"/>`;
  const sail = `<path d="M${x0 + sailAt} ${y} L${x0 + sailAt + 2} ${y - 7} H${x0 + sailAt + 11} L${x0 + sailAt + 12} ${y} Z"/>`;
  const rudder = `<path d="M${x0 + 1} ${y + h / 2} L${x0 - 3} ${y - 3} L${x0 + 4} ${y} Z"/>`;
  return hull + sail + rudder;
}

const PROFILES: Record<ShipKind, Profile> = {
  carrier: { len: 88, fb: 8, rake: 6, flat: true, blocks: [[58, 8, 11]], masts: [[62, 16]] },
  amphib: { len: 82, fb: 10, rake: 4, flat: true, blocks: [[50, 10, 10]], masts: [[55, 15]] },
  cruiser: {
    len: 82,
    fb: 6,
    blocks: [
      [28, 22, 9],
      [52, 8, 6],
    ],
    masts: [
      [36, 17],
      [56, 12],
    ],
    guns: [68, 12],
  },
  destroyer: {
    len: 76,
    fb: 6,
    blocks: [
      [26, 20, 9],
      [48, 7, 5],
    ],
    masts: [[34, 16]],
    guns: [62],
  },
  frigate: {
    len: 68,
    fb: 6,
    blocks: [
      [10, 12, 6],
      [24, 16, 9],
    ],
    masts: [[30, 14]],
    guns: [56],
  },
  corvette: { len: 56, fb: 5, blocks: [[16, 18, 8]], masts: [[24, 12]], guns: [45] },
  fast_attack: { len: 40, fb: 4, rake: 8, blocks: [[12, 12, 6]], masts: [[17, 9]], guns: [32] },
  patrol: { len: 58, fb: 6, blocks: [[22, 18, 8]], masts: [[30, 12]], guns: [48] },
  mcm: { len: 46, fb: 5, blocks: [[18, 14, 8]], masts: [[24, 12]], cargo: [[4, 8, 4]] },
  landing: {
    len: 78,
    fb: 8,
    rake: 3,
    blocks: [[4, 16, 12]],
    masts: [[10, 14]],
    cargo: [[24, 30, 3]],
  },
  oiler: {
    len: 86,
    fb: 7,
    blocks: [
      [4, 14, 11],
      [40, 4, 12],
      [52, 4, 12],
    ],
    masts: [[10, 16]],
  },
  stores: {
    len: 80,
    fb: 8,
    blocks: [[4, 14, 11]],
    masts: [
      [10, 15],
      [40, 14],
      [60, 14],
    ],
  },
  tug: { len: 34, fb: 5, rake: 6, blocks: [[10, 12, 9]], masts: [[16, 10]] },
  survey: {
    len: 56,
    fb: 6,
    blocks: [[14, 22, 8]],
    masts: [
      [20, 16],
      [30, 14],
      [38, 12],
    ],
  },
  container: {
    len: 88,
    fb: 7,
    blocks: [[4, 10, 12]],
    cargo: [
      [16, 14, 7],
      [31, 14, 7],
      [46, 14, 7],
      [61, 14, 6],
    ],
  },
  bulk: {
    len: 86,
    fb: 7,
    blocks: [[4, 10, 11]],
    cargo: [
      [20, 9, 3],
      [34, 9, 3],
      [48, 9, 3],
      [62, 9, 3],
    ],
  },
  tanker: { len: 88, fb: 5, blocks: [[4, 10, 11]], cargo: [[20, 60, 2]] },
  roro: {
    len: 82,
    fb: 6,
    rake: 4,
    blocks: [
      [4, 74, 12],
      [60, 10, 15],
    ],
  },
  trawler: {
    len: 40,
    fb: 6,
    rake: 6,
    blocks: [[22, 10, 8]],
    masts: [
      [8, 12],
      [28, 12],
    ],
  },
  supply: { len: 64, fb: 6, blocks: [[46, 12, 10]], masts: [[50, 14]], cargo: [[6, 30, 2]] },
  ship: { len: 66, fb: 6, blocks: [[22, 18, 8]], masts: [[30, 12]] },
};

function subProfile(kind: SubKind): string {
  switch (kind) {
    case "sub_nuclear":
      return subSvg(84, 9, 26);
    case "sub_midget":
      return subSvg(38, 6, 12);
    default:
      return subSvg(66, 8, 22);
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

/** A small APP-6 frame in the corner: affiliation by shape, not only by colour. */
function badge(aff: Affiliation, x: number, y: number, scale: number): string {
  return `<g transform="translate(${x} ${y}) scale(${scale})" fill="${rgb(AFFILIATION_COLOUR[aff])}" stroke="#0b1b2b" stroke-width="6">${frameSvg(aff, "surface")}</g>`;
}

const cache = new Map<string, string>();

/** Canvas size of a figure's icon, px: profiles are cropped to the strip they occupy. */
export function figureCanvas(kind: FigureKind): [number, number] {
  return figureShape(kind) === "profile" ? [96, 36] : [64, 64];
}

/** Profiles are drawn with their heights stretched 1.6 × about the waterline (recognition-chart
 * style), so a long, low hull still reads at map size; then cropped to y 6–42. */
const PROFILE_STRETCH = `matrix(1 0 0 1.6 0 ${-0.6 * WL})`;
const PROFILE_VIEWBOX = "0 6 96 36";

/**
 * An SVG data URL for a figure in its affiliation colour. Profiles face right, or left when the
 * unit heads west (`facingLeft`). Cached by kind, affiliation, selection and facing.
 */
export function figureUrl(
  kind: FigureKind,
  aff: Affiliation,
  selected: boolean,
  facingLeft = false,
): string {
  const key = `${kind}-${aff}-${selected}-${facingLeft}`;
  const hit = cache.get(key);
  if (hit !== undefined) return hit;
  const shape = figureShape(kind);
  const [w, h] = figureCanvas(kind);
  let body: string;
  if (shape === "profile") {
    body = kind.startsWith("sub")
      ? subProfile(kind as SubKind)
      : profileSvg(PROFILES[kind as ShipKind]);
  } else if (shape === "topdown") {
    body = TOPDOWN[kind as AirKind];
  } else {
    body = LAND[kind as LandKind];
  }
  const flip = facingLeft && shape === "profile" ? ` transform="matrix(-1 0 0 1 ${w} 0)"` : "";
  if (shape === "profile") body = `<g transform="${PROFILE_STRETCH}">${body}</g>`;
  const viewBox = shape === "profile" ? PROFILE_VIEWBOX : `0 0 ${w} ${h}`;
  const stroke = selected ? "#ffffff" : "#0b1b2b";
  const sw = selected ? 2.5 : 1.5;
  const mark = shape === "profile" ? badge(aff, 1, 7, 0.2) : badge(aff, 0, 0, 0.25);
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="${viewBox}"><g fill="${rgb(AFFILIATION_COLOUR[aff])}" stroke="${stroke}" stroke-width="${sw}" stroke-linejoin="round"${flip}>${body}</g>${mark}</svg>`;
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
