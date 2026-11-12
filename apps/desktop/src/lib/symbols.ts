// Our own unit symbology, following the public APP-6 / MIL-STD-2525 conventions: frame shape
// encodes affiliation (so it reads without colour; colour-blind-safe) and domain (sea surface =
// full frame, air = upper half, subsurface = lower half, land = full frame with a rectangular
// friend). Colour reinforces affiliation. Drawn here from scratch; not copied from any product.

import type { Domain } from "./bindings";

export type Affiliation = "friend" | "hostile" | "neutral" | "unknown";

export const AFFILIATION_COLOUR: Record<Affiliation, [number, number, number]> = {
  friend: [128, 224, 255],
  hostile: [255, 128, 128],
  neutral: [170, 255, 170],
  unknown: [255, 255, 128],
};

// Full frames on a 64×64 canvas (sea surface; land except the friend rectangle).
const FULL: Record<Affiliation, string> = {
  friend: `<circle cx="32" cy="32" r="22"/>`,
  hostile: `<path d="M32 6 L58 32 L32 58 L6 32 Z"/>`,
  neutral: `<rect x="11" y="11" width="42" height="42"/>`,
  unknown: `<path d="M32 10 a11 11 0 0 1 11 11 a11 11 0 0 1 0 22 a11 11 0 0 1 -22 0 a11 11 0 0 1 0 -22 a11 11 0 0 1 11 -11 Z"/>`,
};

// Upper-half frames (air) closed by a flat base at y = 44; subsurface uses the mirror image.
const UPPER: Record<Affiliation, string> = {
  friend: `M8 44 A24 24 0 0 1 56 44 Z`,
  hostile: `M8 44 L32 14 L56 44 Z`,
  neutral: `M10 44 V14 H54 V44 Z`,
  unknown: `M10 44 A11 11 0 0 1 21 26 A11 11 0 0 1 43 26 A11 11 0 0 1 54 44 Z`,
};

/** The frame's SVG elements for an affiliation and domain. */
export function frameSvg(aff: Affiliation, domain: Domain): string {
  switch (domain) {
    case "air":
      return `<path d="${UPPER[aff]}"/>`;
    case "subsurface":
      // Mirror the upper half about the horizontal centre line (y → 64 − y).
      return `<path transform="matrix(1 0 0 -1 0 64)" d="${UPPER[aff]}"/>`;
    case "land":
      return aff === "friend" ? `<rect x="5" y="16" width="54" height="32"/>` : FULL[aff];
    default:
      return FULL[aff];
  }
}

/**
 * Function code drawn inside the frame (docs/behaviour-specs/symbology.md). APP-6 shows a sea
 * unit's function as a short text code (hull and role abbreviations) and air roles as letters;
 * we draw plain text codes for every domain. Known generic classes have their own code; any other
 * platform falls back to its category; an unclassified contact shows none.
 */
const CLASS_CODE: Record<string, string> = {
  "GEN-CV": "CV",
  "GEN-LHD": "LHD",
  "GEN-CG": "CG",
  "GEN-DDG": "DDG",
  "GEN-FFG": "FFG",
  "GEN-FFG-ASW": "FFG",
  "GEN-FFL": "FFL",
  "GEN-PGG": "PG",
  "GEN-OPV": "OPV",
  "GEN-MCMV": "MCM",
  "GEN-LST": "LST",
  "GEN-AOR": "AOR",
  "GEN-AKE": "AKE",
  "GEN-ATF": "ATF",
  "GEN-AGS": "AGS",
  "GEN-AGI": "AGI",
  "GEN-MV-CONTAINER": "MV",
  "GEN-MV-BULK": "MV",
  "GEN-MV-TANKER": "TK",
  "GEN-MV-RORO": "MV",
  "GEN-MV-TRAWLER": "FV",
  "GEN-MV-OSV": "OSV",
  "GEN-SSN": "SSN",
  "GEN-SSGN": "SSGN",
  "GEN-SSK": "SSK",
  "GEN-SSK-AIP": "SSK",
  "GEN-SSM": "SSM",
  "GEN-MPA-TP": "P",
  "GEN-MPA-JET": "P",
  "GEN-AEW": "W",
  "GEN-TANKER": "K",
  "GEN-FTR-MR": "F",
  "GEN-FTR-AS": "F",
  "GEN-FTR-CV": "F",
  "GEN-FTR-LIGHT": "F",
  "GEN-BMR": "B",
  "GEN-HELO-ASW": "H",
  "GEN-HELO-UTIL": "H",
  "GEN-HELO-HEAVY": "H",
  "GEN-HELO-LIGHT": "H",
  "GEN-UAV-MALE": "U",
  "GEN-UAV-VTOL": "U",
  "GEN-SHORE-ASHM": "ASM",
  "GEN-SHORE-SAM-LR": "SAM",
  "GEN-SHORE-SAM-MR": "SAM",
  "GEN-SITE-RADAR": "RDR",
  "GEN-AIRBASE-MAIN": "AB",
  "GEN-AIRFIELD-FWD": "AB",
  "GEN-PORT-NAVAL": "PT",
  "GEN-PORT-COMMERCIAL": "PT",
};

const CATEGORY_CODE: Record<string, string> = {
  submarine: "SS",
  fighter: "F",
  helicopter: "H",
  mpa: "P",
  aew: "W",
  tanker: "K",
  uav: "U",
  merchant: "MV",
  auxiliary: "A",
  shore_battery: "BTY",
  sensor_site: "RDR",
  airbase: "AB",
  port: "PT",
  // A surface combatant of unknown type shows the frame alone: no code is better than a wrong one.
};

/** The code inside the frame: by class, else by category, else none. */
export function functionCode(platform?: string | null, category?: string | null): string {
  return (
    (platform ? CLASS_CODE[platform] : undefined) ??
    (category ? CATEGORY_CODE[category] : undefined) ??
    ""
  );
}

/** The code as SVG text, centred in the frame's interior for the domain. */
function codeSvg(code: string, domain: Domain): string {
  if (!code) return "";
  const size = code.length <= 2 ? 24 : code.length === 3 ? 17 : 13;
  // Interior centre: full frames 32; the air half-frame sits higher, the subsurface one lower.
  const y = domain === "air" ? 34 : domain === "subsurface" ? 31 : 33;
  return `<text x="32" y="${y}" text-anchor="middle" dominant-baseline="central" font-family="Arial, Helvetica, sans-serif" font-weight="700" font-size="${size}" fill="#0b1b2b" stroke="none">${code}</text>`;
}

function rgb([r, g, b]: [number, number, number]) {
  return `rgb(${r},${g},${b})`;
}

const urlCache = new Map<string, string>();

/** An SVG data URL for a frame and its function code. Cached: the map asks for it once per unit
 * per frame. */
export function symbolUrl(
  aff: Affiliation,
  selected: boolean,
  domain: Domain = "surface",
  code = "",
): string {
  const key = `${aff}-${domain}-${selected}-${code}`;
  let url = urlCache.get(key);
  if (url === undefined) {
    url = buildSymbolUrl(aff, selected, domain, code);
    urlCache.set(key, url);
  }
  return url;
}

function buildSymbolUrl(aff: Affiliation, selected: boolean, domain: Domain, code: string): string {
  const fill = rgb(AFFILIATION_COLOUR[aff]);
  const stroke = selected ? "#ffffff" : "#0b1b2b";
  const sw = selected ? 5 : 3;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><g fill="${fill}" fill-opacity="0.9" stroke="${stroke}" stroke-width="${sw}">${frameSvg(aff, domain)}</g>${codeSvg(code, domain)}</svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}
