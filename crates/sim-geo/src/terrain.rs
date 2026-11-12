//! Terrain and bathymetry packs (`docs/models/terrain-and-bathymetry.md`, Phase 4 slice 2).
//!
//! A pack is a regular latitude/longitude grid of heights in whole metres (positive up, so the
//! sea floor is negative), built offline from GEBCO by `nws-batch terrain build`. Grid positions
//! are integers in tenths of an arc-second, and sampling uses only `+ − × ÷`, `floor` and `%`, so
//! a height is bit-identical on every OS (ADR-0003, ADR-0006).
//!
//! File layout (all integers little-endian):
//!
//! | Bytes | Content |
//! |---|---|
//! | 8 | magic `NWSTERR\0` |
//! | 2 | format version (1) |
//! | 4 | header length `n` |
//! | n | header, UTF-8 JSON ([`PackHeader`]) |
//! | rest | zlib-deflated heights: `i16`, rows south to north, each row west to east and delta-coded (each value minus the one to its west, wrapping) |
//!
//! A pack is identified by its name and the first 16 hex digits of the file's SHA-256, so two
//! different files never share an id. The id of the terrain in use is part of the state hash.

use crate::GeoPoint;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

/// First bytes of every pack file.
pub const PACK_MAGIC: &[u8; 8] = b"NWSTERR\0";
/// Increment on any change to the file layout, and keep older versions loading.
pub const PACK_VERSION: u16 = 1;
/// Grid units per degree: tenths of an arc-second.
pub const UNITS_PER_DEGREE: i64 = 36_000;
/// File extension of a pack.
pub const PACK_EXTENSION: &str = "nwsterrain";
/// A grid cell with no height (in [`Terrain::sample_grid`]).
pub const NO_DATA: i16 = i16::MIN;

const FULL_CIRCLE: i64 = 360 * UNITS_PER_DEGREE;
/// Largest grid a pack may hold (cells), so a corrupt header cannot exhaust memory.
const MAX_CELLS: u64 = 1_000_000_000;

/// Why a pack could not be read or written.
#[derive(Debug)]
pub enum PackError {
    NotAPack,
    FutureVersion(u16),
    BadHeader(String),
    BadData(String),
    Io(std::io::Error),
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackError::NotAPack => f.write_str("not a terrain pack"),
            PackError::FutureVersion(v) => write!(
                f,
                "terrain pack format {v} is newer than this build reads ({PACK_VERSION})"
            ),
            PackError::BadHeader(m) => write!(f, "terrain pack header: {m}"),
            PackError::BadData(m) => write!(f, "terrain pack data: {m}"),
            PackError::Io(e) => write!(f, "terrain pack: {e}"),
        }
    }
}

impl std::error::Error for PackError {}

impl From<std::io::Error> for PackError {
    fn from(e: std::io::Error) -> Self {
        PackError::Io(e)
    }
}

/// What a pack holds and where. Positions are cell centres in tenths of an arc-second.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PackHeader {
    /// Short name, such as `gebco2020-par-15s`.
    pub name: String,
    /// The data set it was built from.
    pub source: String,
    /// The credit the source's licence asks to be shown.
    pub credit: String,
    /// Latitude of the centre of the southernmost row.
    pub lat0: i32,
    /// Longitude of the centre of the westernmost column.
    pub lon0: i32,
    /// Spacing between cell centres, both ways.
    pub cell: u32,
    pub rows: u32,
    pub cols: u32,
    /// Lowest and highest height in the pack, metres.
    pub min_m: i16,
    pub max_m: i16,
}

impl PackHeader {
    fn check(&self) -> Result<(), PackError> {
        let bad = |m: &str| Err(PackError::BadHeader(m.to_owned()));
        if self.name.is_empty() || self.name.contains(['@', '+', '/', '\\']) {
            return bad("the name must be non-empty, without @ + / or \\");
        }
        if self.cell == 0 || self.rows == 0 || self.cols == 0 {
            return bad("empty grid");
        }
        if u64::from(self.rows) * u64::from(self.cols) > MAX_CELLS {
            return bad("grid too large");
        }
        let cell = i64::from(self.cell);
        let south = i64::from(self.lat0);
        let north = south + (i64::from(self.rows) - 1) * cell;
        if south < -90 * UNITS_PER_DEGREE || north > 90 * UNITS_PER_DEGREE {
            return bad("latitude outside ±90°");
        }
        if i64::from(self.cols) * cell > FULL_CIRCLE {
            return bad("wider than 360°");
        }
        let lon0 = i64::from(self.lon0);
        if !(-180 * UNITS_PER_DEGREE..180 * UNITS_PER_DEGREE).contains(&lon0) {
            return bad("longitude outside −180°..180°");
        }
        Ok(())
    }

    /// Whether the columns go all the way round the earth.
    #[must_use]
    pub fn wraps(&self) -> bool {
        i64::from(self.cols) * i64::from(self.cell) == FULL_CIRCLE
    }

    /// Cell spacing in arc-seconds.
    #[must_use]
    pub fn cell_arcsec(&self) -> f64 {
        f64::from(self.cell) / 10.0
    }

    /// South, west, north and east edges of the covered area, degrees.
    #[must_use]
    pub fn bounds_deg(&self) -> [f64; 4] {
        let u = UNITS_PER_DEGREE as f64;
        let half = f64::from(self.cell) / 2.0;
        let south = f64::from(self.lat0) - half;
        let north = south + f64::from(self.rows) * f64::from(self.cell);
        let west = f64::from(self.lon0) - half;
        let east = west + f64::from(self.cols) * f64::from(self.cell);
        [
            (south / u).max(-90.0),
            west / u,
            (north / u).min(90.0),
            east / u,
        ]
    }
}

/// Writes a pack file from a grid of heights (`rows × cols`, rows south to north).
pub fn encode(header: &PackHeader, heights: &[i16]) -> Result<Vec<u8>, PackError> {
    header.check()?;
    let cols = header.cols as usize;
    if heights.len() != header.rows as usize * cols {
        return Err(PackError::BadData(format!(
            "{} heights for a {} × {} grid",
            heights.len(),
            header.rows,
            header.cols
        )));
    }
    let json = serde_json::to_vec(header).map_err(|e| PackError::BadHeader(e.to_string()))?;
    let mut out = Vec::with_capacity(heights.len());
    out.extend_from_slice(PACK_MAGIC);
    out.extend_from_slice(&PACK_VERSION.to_le_bytes());
    out.extend_from_slice(
        &u32::try_from(json.len())
            .map_err(|_| PackError::BadHeader("header too long".into()))?
            .to_le_bytes(),
    );
    out.extend_from_slice(&json);
    let mut z = flate2::write::ZlibEncoder::new(out, flate2::Compression::best());
    let mut row_bytes = Vec::with_capacity(cols * 2);
    for row in heights.chunks_exact(cols) {
        row_bytes.clear();
        let mut prev = 0i16;
        for &h in row {
            row_bytes.extend_from_slice(&h.wrapping_sub(prev).to_le_bytes());
            prev = h;
        }
        z.write_all(&row_bytes)?;
    }
    Ok(z.finish()?)
}

/// SHA-256 of `bytes`, as lowercase hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// One loaded pack.
pub struct TerrainPack {
    header: PackHeader,
    id: String,
    sha256: String,
    heights: Vec<i16>,
}

impl std::fmt::Debug for TerrainPack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerrainPack")
            .field("id", &self.id)
            .field("rows", &self.header.rows)
            .field("cols", &self.header.cols)
            .finish_non_exhaustive()
    }
}

impl TerrainPack {
    /// Reads a pack file's bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, PackError> {
        let rest = bytes.strip_prefix(PACK_MAGIC).ok_or(PackError::NotAPack)?;
        let (version, rest) = rest.split_first_chunk::<2>().ok_or(PackError::NotAPack)?;
        let version = u16::from_le_bytes(*version);
        if version > PACK_VERSION {
            return Err(PackError::FutureVersion(version));
        }
        let (len, rest) = rest.split_first_chunk::<4>().ok_or(PackError::NotAPack)?;
        let len = u32::from_le_bytes(*len) as usize;
        if rest.len() < len {
            return Err(PackError::BadHeader("truncated".into()));
        }
        let (json, data) = rest.split_at(len);
        let header: PackHeader =
            serde_json::from_slice(json).map_err(|e| PackError::BadHeader(e.to_string()))?;
        header.check()?;
        let cols = header.cols as usize;
        let cells = header.rows as usize * cols;
        let mut raw = Vec::with_capacity(cells * 2);
        flate2::read::ZlibDecoder::new(data)
            .take(cells as u64 * 2 + 1)
            .read_to_end(&mut raw)?;
        if raw.len() != cells * 2 {
            return Err(PackError::BadData(format!(
                "{} bytes of heights, expected {}",
                raw.len(),
                cells * 2
            )));
        }
        let mut heights = Vec::with_capacity(cells);
        for row in raw.chunks_exact(cols * 2) {
            let mut prev = 0i16;
            for d in row.as_chunks::<2>().0 {
                prev = prev.wrapping_add(i16::from_le_bytes(*d));
                heights.push(prev);
            }
        }
        let sha256 = sha256_hex(bytes);
        Ok(Self {
            id: format!("{}@{}", header.name, &sha256[..16]),
            header,
            sha256,
            heights,
        })
    }

    /// Reads a pack file.
    pub fn load(path: &Path) -> Result<Self, PackError> {
        Self::decode(&std::fs::read(path)?)
    }

    /// `name@` and the first 16 hex digits of the file's SHA-256.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// SHA-256 of the whole file, lowercase hex.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[must_use]
    pub fn header(&self) -> &PackHeader {
        &self.header
    }

    fn at(&self, row: usize, col: usize) -> f64 {
        f64::from(self.heights[row * self.header.cols as usize + col])
    }

    /// Height at `p` in metres (positive up), bilinear between the four nearest cell centres.
    /// `None` outside the pack. Within half a cell of the outer rows and columns the edge value
    /// holds.
    #[must_use]
    pub fn height_m(&self, p: GeoPoint) -> Option<f64> {
        let h = &self.header;
        let u = UNITS_PER_DEGREE as f64;
        let cell = f64::from(h.cell);
        let rows = f64::from(h.rows);
        let cols = f64::from(h.cols);
        let y = (p.lat_deg * u - f64::from(h.lat0)) / cell;
        if !(-0.5..=rows - 0.5).contains(&y) {
            return None;
        }
        let mut x = (p.lon_deg * u - f64::from(h.lon0)) / cell;
        let wraps = h.wraps();
        if wraps {
            x %= cols;
            if x < 0.0 {
                x += cols;
            }
        } else {
            // A pack east of the antimeridian may be asked about with longitudes near −180.
            if x < -0.5 {
                x += 360.0 * u / cell;
            }
            if !(-0.5..=cols - 0.5).contains(&x) {
                return None;
            }
        }
        let y = y.clamp(0.0, rows - 1.0);
        let r0 = libm::floor(y);
        let fy = y - r0;
        let r0 = r0 as usize;
        let r1 = (r0 + 1).min(h.rows as usize - 1);
        let (c0, c1, fx) = if wraps {
            let c = libm::floor(x);
            let c0 = (c as usize).min(h.cols as usize - 1);
            (c0, (c0 + 1) % h.cols as usize, x - c)
        } else {
            let x = x.clamp(0.0, cols - 1.0);
            let c = libm::floor(x);
            let c0 = c as usize;
            (c0, (c0 + 1).min(h.cols as usize - 1), x - c)
        };
        let south = self.at(r0, c0) + (self.at(r0, c1) - self.at(r0, c0)) * fx;
        let north = self.at(r1, c0) + (self.at(r1, c1) - self.at(r1, c0)) * fx;
        Some(south + (north - south) * fy)
    }
}

/// The terrain a run uses: packs looked up finest first, so a regional pack overrides the
/// coarse global layer where it has data.
pub struct Terrain {
    packs: Vec<Arc<TerrainPack>>,
    id: String,
}

impl std::fmt::Debug for Terrain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terrain").field("id", &self.id).finish()
    }
}

impl Terrain {
    /// Orders the packs finest first (then by id, so the order never depends on how they were
    /// found). The id joins the pack ids with `+`. `None` for no packs.
    #[must_use]
    pub fn new(mut packs: Vec<Arc<TerrainPack>>) -> Option<Self> {
        packs.sort_by(|a, b| {
            a.header
                .cell
                .cmp(&b.header.cell)
                .then_with(|| a.id.cmp(&b.id))
        });
        packs.dedup_by(|a, b| a.id == b.id);
        if packs.is_empty() {
            return None;
        }
        let id = packs
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>()
            .join("+");
        Some(Self { packs, id })
    }

    /// Identifies this set of packs; recorded in the state hash and in exercise records.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn packs(&self) -> &[Arc<TerrainPack>] {
        &self.packs
    }

    /// Height at `p` in metres, positive up, from the finest pack that covers it.
    #[must_use]
    pub fn height_m(&self, p: GeoPoint) -> Option<f64> {
        self.packs.iter().find_map(|k| k.height_m(p))
    }

    /// Water depth at `p` in metres (0 on land).
    #[must_use]
    pub fn depth_m(&self, p: GeoPoint) -> Option<f64> {
        self.height_m(p).map(|h| (-h).max(0.0))
    }

    /// Heights for display: `cols` cell centres from `west` to `east` along each latitude in
    /// `lats` (any order, typically north first and evenly spaced in Web Mercator), rounded to
    /// whole metres, [`NO_DATA`] where no pack covers.
    #[must_use]
    pub fn sample_grid(&self, lats: &[f64], west: f64, east: f64, cols: u32) -> Vec<i16> {
        let mut east = east;
        if east < west {
            east += 360.0;
        }
        let dx = (east - west) / f64::from(cols.max(1));
        let mut out = Vec::with_capacity(lats.len() * cols as usize);
        for &lat in lats {
            for c in 0..cols {
                let lon = west + (f64::from(c) + 0.5) * dx;
                let h = self.height_m(GeoPoint::new(lat.clamp(-90.0, 90.0), lon));
                out.push(h.map_or(NO_DATA, |h| {
                    libm::round(h).clamp(f64::from(i16::MIN + 1), f64::from(i16::MAX)) as i16
                }));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(lat0: i32, lon0: i32, cell: u32, rows: u32, cols: u32) -> PackHeader {
        PackHeader {
            name: "test".into(),
            source: "unit test".into(),
            credit: "none".into(),
            lat0,
            lon0,
            cell,
            rows,
            cols,
            min_m: -100,
            max_m: 100,
        }
    }

    const DEG: i32 = UNITS_PER_DEGREE as i32;

    #[test]
    fn round_trips_and_ids_by_content() {
        let h = header(10 * DEG, 120 * DEG, DEG as u32, 3, 4);
        let heights: Vec<i16> = vec![-100, -50, 0, 50, -90, -40, 10, 60, i16::MIN + 1, 7, 8, 100];
        let bytes = encode(&h, &heights).unwrap();
        let pack = TerrainPack::decode(&bytes).unwrap();
        assert_eq!(pack.header(), &h);
        assert_eq!(pack.heights, heights);
        assert!(pack.id().starts_with("test@"));
        assert_eq!(pack.id().len(), "test@".len() + 16);
        let mut other = heights.clone();
        other[0] = -101;
        let pack2 = TerrainPack::decode(&encode(&h, &other).unwrap()).unwrap();
        assert_ne!(pack.id(), pack2.id());
    }

    #[test]
    fn rejects_damaged_files() {
        let h = header(0, 0, DEG as u32, 2, 2);
        let bytes = encode(&h, &[1, 2, 3, 4]).unwrap();
        assert!(matches!(
            TerrainPack::decode(b"nonsense"),
            Err(PackError::NotAPack)
        ));
        let mut future = bytes.clone();
        future[8] = 9;
        assert!(matches!(
            TerrainPack::decode(&future),
            Err(PackError::FutureVersion(9))
        ));
        assert!(TerrainPack::decode(&bytes[..bytes.len() - 3]).is_err());
        assert!(encode(&h, &[1, 2, 3]).is_err());
    }

    #[test]
    fn bilinear_between_cell_centres() {
        // Cell centres at 10°N/11°N and 120°E/121°E.
        let h = header(10 * DEG, 120 * DEG, DEG as u32, 2, 2);
        let pack = TerrainPack::decode(&encode(&h, &[-100, 0, 100, 300]).unwrap()).unwrap();
        let at = |lat, lon| pack.height_m(GeoPoint::new(lat, lon));
        assert_eq!(at(10.0, 120.0), Some(-100.0));
        assert_eq!(at(11.0, 121.0), Some(300.0));
        assert_eq!(at(10.0, 120.5), Some(-50.0));
        assert_eq!(at(10.5, 120.5), Some(75.0));
        // Half a cell beyond the edge holds the edge value; further out is not covered.
        assert_eq!(at(9.6, 120.0), Some(-100.0));
        assert_eq!(at(9.4, 120.0), None);
        assert_eq!(at(10.0, 121.6), None);
    }

    #[test]
    fn global_grid_wraps_at_the_antimeridian() {
        // 4 columns of 90° centred on −135, −45, 45, 135.
        let h = header(0, -135 * DEG, 90 * DEG as u32, 1, 4);
        assert!(h.wraps());
        let pack = TerrainPack::decode(&encode(&h, &[0, 100, 200, 400]).unwrap()).unwrap();
        let at = |lon| pack.height_m(GeoPoint::new(0.0, lon)).unwrap();
        assert_eq!(at(135.0), 400.0);
        // Half way from 135°E to 225°E (= −135°): between the last and first columns.
        assert_eq!(at(180.0), 200.0);
        assert_eq!(at(-180.0), 200.0);
        assert_eq!(at(-157.5), 100.0);
    }

    /// Cross-OS guard (ADR-0003): 20,000 samples over an uneven synthetic grid, wrapped round the
    /// antimeridian, folded bit for bit into one number. CI runs this on Windows and macOS; the
    /// constant only changes if the sampling maths does.
    #[test]
    fn sampling_is_bit_identical_everywhere() {
        let (rows, cols) = (91u32, 180u32);
        let heights: Vec<i16> = (0..rows * cols)
            .map(|i| {
                let (r, c) = (i / cols, i % cols);
                ((r * 7919 + c * 104_729 + (r * c) % 613) % 16_000) as i16 - 9_000
            })
            .collect();
        // 2° cells, centres from 90°S and from 179°W: wraps round the world.
        let h = header(-90 * DEG, -179 * DEG, 2 * DEG as u32, rows, cols);
        assert!(h.wraps());
        let pack = TerrainPack::decode(&encode(&h, &heights).unwrap()).unwrap();
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut fold = 0xcbf2_9ce4_8422_2325u64;
        for _ in 0..20_000 {
            let p = GeoPoint::new(next() * 180.0 - 90.0, next() * 360.0 - 180.0);
            let v = pack.height_m(p).unwrap();
            fold = (fold ^ v.to_bits()).wrapping_mul(0x0100_0000_01b3);
        }
        assert_eq!(
            fold, 0xb702_9321_00e9_7803,
            "sampling changed: {fold:#018x}"
        );
    }

    #[test]
    fn finest_pack_wins_and_order_does_not_matter() {
        let coarse = Arc::new(
            TerrainPack::decode(
                &encode(
                    &PackHeader {
                        name: "coarse".into(),
                        ..header(0, 0, 10 * DEG as u32, 2, 2)
                    },
                    &[-1000; 4],
                )
                .unwrap(),
            )
            .unwrap(),
        );
        let fine = Arc::new(
            TerrainPack::decode(
                &encode(
                    &PackHeader {
                        name: "fine".into(),
                        ..header(DEG, DEG, DEG as u32, 2, 2)
                    },
                    &[-5; 4],
                )
                .unwrap(),
            )
            .unwrap(),
        );
        let a = Terrain::new(vec![coarse.clone(), fine.clone()]).unwrap();
        let b = Terrain::new(vec![fine, coarse]).unwrap();
        assert_eq!(a.id(), b.id());
        assert!(a.id().starts_with("fine@"));
        assert_eq!(a.height_m(GeoPoint::new(1.5, 1.5)), Some(-5.0));
        assert_eq!(a.depth_m(GeoPoint::new(5.0, 5.0)), Some(1000.0));
        assert_eq!(a.height_m(GeoPoint::new(50.0, 5.0)), None);
        assert!(Terrain::new(vec![]).is_none());
        let grid = a.sample_grid(&[1.5, 0.5], 0.0, 2.0, 2);
        assert_eq!(grid, vec![-5, -5, -5, -5]);
    }
}
