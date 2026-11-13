//! Land mask and sea-route planning (`docs/models/land-avoidance.md`).
//!
//! The coastline (Natural Earth 1:50m land, public domain) is rasterised once into a global grid of
//! `cells_per_degree`² cells per square degree. A cell is land if its centre lies inside a land
//! polygon (even–odd rule, so lakes cut out of land stay water). Only `+ − × ÷`, floor and ceil are
//! used, so the mask is bit-identical on every OS (ADR-0003).
//!
//! Route planning runs A* over water cells (8-connected, octile integer costs, a penalty for cells
//! next to land to keep clearance), then shortens the cell path into a few straight legs that stay
//! clear of the coast.

use crate::terrain::Terrain;
use crate::GeoPoint;
use std::cell::RefCell;
use std::collections::{BTreeMap, BinaryHeap};
use std::sync::{Arc, OnceLock};

/// Raster resolution of the bundled mask: 1/30° (≈ 3.7 km at the equator).
pub const EMBEDDED_CELLS_PER_DEGREE: u32 = 30;
/// Identifier of the bundled mask, recorded in the simulation state hash.
pub const EMBEDDED_MASK_ID: &str = "ne_50m_land@30";

/// Cost multiplier for crossing a cell that touches land (keeps routes off the coast).
const COASTAL_PENALTY: u32 = 4;
/// Largest search region, in cells, before a plan is refused as too long.
const MAX_REGION_CELLS: usize = 9_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    StartOnLand,
    WaypointOnLand,
    NoSeaRoute,
    TooFar,
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PlanError::StartOnLand => "the unit is on land",
            PlanError::WaypointOnLand => "a waypoint is on land",
            PlanError::NoSeaRoute => "no sea route to the waypoint",
            PlanError::TooFar => "the leg is too long to plan around land",
        })
    }
}

/// Water a unit must not plan through: shallower than `min_depth_m` (its draft plus under-keel
/// clearance) on the terrain packs (`docs/models/shallow-water.md`).
#[derive(Debug, Clone, Copy)]
pub struct DepthLimit<'a> {
    pub terrain: &'a Terrain,
    pub min_depth_m: f64,
}

/// A planned route.
#[derive(Debug, Clone, PartialEq)]
pub struct RoutePlan {
    /// The given waypoints, in order, with turning points added where legs were replanned.
    pub waypoints: Vec<GeoPoint>,
    /// Legs replanned (round land or shallow water).
    pub replanned: usize,
    /// Legs with no route deep enough for the depth limit: planned round land only, so the unit
    /// may meet water too shallow for it.
    pub shallow_legs: usize,
}

/// Cells too shallow for one leg, worked out as the search reaches them. Cells within 2 of the
/// leg's ends are exempt, so legs can start and finish in harbours and inshore anchorages.
struct Shallow<'a> {
    limit: DepthLimit<'a>,
    start: (i64, i64),
    goal: (i64, i64),
    cache: RefCell<BTreeMap<(i64, i64), bool>>,
}

impl Shallow<'_> {
    fn exempt(&self, (r, c): (i64, i64)) -> bool {
        let near = |(r0, c0): (i64, i64)| (r - r0).abs().max((c - c0).abs()) <= 2;
        near(self.start) || near(self.goal)
    }
}

/// A global land/water raster.
pub struct LandMask {
    id: String,
    per_deg: f64,
    cols: i64,
    rows: i64,
    stride: usize,
    land: Vec<u64>,
    /// Land dilated by one cell: water cells here are next to the coast.
    near_land: Vec<u64>,
}

impl std::fmt::Debug for LandMask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LandMask")
            .field("id", &self.id)
            .field("cols", &self.cols)
            .field("rows", &self.rows)
            .finish_non_exhaustive()
    }
}

fn collect_rings(geometry: &serde_json::Value, out: &mut Vec<Vec<(f64, f64)>>) {
    let polys: Vec<&serde_json::Value> = match geometry.get("type").and_then(|t| t.as_str()) {
        Some("Polygon") => geometry.get("coordinates").into_iter().collect(),
        Some("MultiPolygon") => geometry
            .get("coordinates")
            .and_then(|c| c.as_array())
            .map(|a| a.iter().collect())
            .unwrap_or_default(),
        _ => vec![],
    };
    for poly in polys {
        for ring in poly.as_array().into_iter().flatten() {
            let pts: Vec<(f64, f64)> = ring
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?)))
                .collect();
            if pts.len() >= 3 {
                out.push(pts);
            }
        }
    }
}

impl LandMask {
    /// Rasterises a GeoJSON FeatureCollection of land polygons.
    pub fn from_geojson(text: &str, cells_per_degree: u32, id: &str) -> Result<Self, String> {
        let doc: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("land GeoJSON: {e}"))?;
        let mut rings = Vec::new();
        for f in doc
            .get("features")
            .and_then(|f| f.as_array())
            .into_iter()
            .flatten()
        {
            if let Some(g) = f.get("geometry") {
                collect_rings(g, &mut rings);
            }
        }
        if rings.is_empty() {
            return Err("land GeoJSON holds no polygons".into());
        }
        let per_deg = f64::from(cells_per_degree);
        let cols = i64::from(cells_per_degree) * 360;
        let rows = i64::from(cells_per_degree) * 180;
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let stride = (cols as usize).div_ceil(64);
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let words = stride * rows as usize;
        let mut mask = Self {
            id: id.to_owned(),
            per_deg,
            cols,
            rows,
            stride,
            land: vec![0; words],
            near_land: vec![0; words],
        };

        // Scanline fill: per row, the x of every edge crossing the row's centre latitude.
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let mut crossings: Vec<Vec<f64>> = vec![Vec::new(); rows as usize];
        for ring in &rings {
            for i in 0..ring.len() {
                let (x1, y1) = ring[i];
                let (x2, y2) = ring[(i + 1) % ring.len()];
                if y1 == y2 {
                    continue;
                }
                let (lo, hi) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
                // Rows whose centre latitude c satisfies lo <= c < hi; row r centre = 90 − (r+0.5)/k.
                let r_first = ((90.0 - hi) * per_deg - 0.5).ceil().max(0.0);
                #[allow(clippy::cast_precision_loss)]
                let r_last = ((90.0 - lo) * per_deg - 0.5).floor().min((rows - 1) as f64);
                let mut r = r_first;
                while r <= r_last {
                    let c = 90.0 - (r + 0.5) / per_deg;
                    if c >= lo && c < hi {
                        let x = x1 + (c - y1) * (x2 - x1) / (y2 - y1);
                        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                        crossings[r as usize].push(x);
                    }
                    r += 1.0;
                }
            }
        }
        for (r, xs) in crossings.iter_mut().enumerate() {
            xs.sort_by(f64::total_cmp);
            for pair in xs.as_chunks::<2>().0 {
                // Columns whose centre lon = −180 + (c+0.5)/k lies in [x0, x1).
                let c0 = ((pair[0] + 180.0) * per_deg - 0.5).ceil().max(0.0);
                #[allow(clippy::cast_precision_loss)]
                let c1 = ((pair[1] + 180.0) * per_deg - 0.5).ceil().min(cols as f64);
                if c1 > c0 {
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    mask.fill_row(r, c0 as usize, c1 as usize);
                }
            }
        }
        mask.dilate();
        Ok(mask)
    }

    fn fill_row(&mut self, row: usize, c0: usize, c1: usize) {
        let base = row * self.stride;
        for c in c0..c1 {
            self.land[base + c / 64] |= 1 << (c % 64);
        }
    }

    /// `near_land` = land grown by one cell in all eight directions.
    fn dilate(&mut self) {
        let s = self.stride;
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let rows = self.rows as usize;
        let mut horiz = vec![0u64; self.land.len()];
        for r in 0..rows {
            for w in 0..s {
                let i = r * s + w;
                let cur = self.land[i];
                let left_carry = if w > 0 { self.land[i - 1] >> 63 } else { 0 };
                let right_carry = if w + 1 < s { self.land[i + 1] << 63 } else { 0 };
                horiz[i] = cur | (cur << 1) | left_carry | (cur >> 1) | right_carry;
            }
        }
        for r in 0..rows {
            for w in 0..s {
                let mut v = horiz[r * s + w];
                if r > 0 {
                    v |= horiz[(r - 1) * s + w];
                }
                if r + 1 < rows {
                    v |= horiz[(r + 1) * s + w];
                }
                self.near_land[r * s + w] = v;
            }
        }
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    fn row_of(&self, lat: f64) -> i64 {
        #[allow(clippy::cast_possible_truncation)]
        let r = ((90.0 - lat) * self.per_deg).floor() as i64;
        r.clamp(0, self.rows - 1)
    }

    fn col_of(&self, lon: f64) -> i64 {
        #[allow(clippy::cast_possible_truncation)]
        let c = ((lon + 180.0) * self.per_deg).floor() as i64;
        c.rem_euclid(self.cols)
    }

    fn bit(&self, bits: &[u64], row: i64, col: i64) -> bool {
        if row < 0 || row >= self.rows {
            return true; // beyond the poles: treat as blocked
        }
        let c = col.rem_euclid(self.cols);
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let (r, c) = (row as usize, c as usize);
        bits[r * self.stride + c / 64] & (1 << (c % 64)) != 0
    }

    fn land_cell(&self, row: i64, col: i64) -> bool {
        self.bit(&self.land, row, col)
    }

    fn near_land_cell(&self, row: i64, col: i64) -> bool {
        self.bit(&self.near_land, row, col)
    }

    /// True if any of 3 × 3 points across the cell is shallower than the limit (land on the
    /// terrain counts). No terrain data there counts as deep enough.
    fn shallow_cell(&self, sh: &Shallow<'_>, row: i64, col: i64) -> bool {
        let key = (row, col.rem_euclid(self.cols));
        if let Some(&v) = sh.cache.borrow().get(&key) {
            return v;
        }
        let centre = self.cell_centre(row, col);
        let step = 1.0 / (3.0 * self.per_deg);
        let mut shallow = false;
        'grid: for i in -1i32..=1 {
            for j in -1i32..=1 {
                let p = GeoPoint::new(
                    (centre.lat_deg + f64::from(i) * step).clamp(-90.0, 90.0),
                    centre.lon_deg + f64::from(j) * step,
                );
                if sh
                    .limit
                    .terrain
                    .height_m(p)
                    .is_some_and(|h| -h < sh.limit.min_depth_m)
                {
                    shallow = true;
                    break 'grid;
                }
            }
        }
        sh.cache.borrow_mut().insert(key, shallow);
        shallow
    }

    /// A cell the route may not enter: land, or (with a depth limit) too shallow away from the
    /// leg's ends.
    fn near_shallow(&self, sh: Option<&Shallow<'_>>, row: i64, col: i64) -> bool {
        let Some(sh) = sh else {
            return false;
        };
        (-1..=1).any(|dr| {
            (-1..=1).any(|dc| {
                let (r, c) = (row + dr, col + dc);
                !sh.exempt((r, c)) && self.shallow_cell(sh, r, c)
            })
        })
    }

    /// A cell to keep clear of where possible: next to land, or (with a depth limit) next to
    /// water too shallow, so turns and dead reckoning keep a margin.
    fn near_hazard(&self, sh: Option<&Shallow<'_>>, row: i64, col: i64) -> bool {
        self.near_land_cell(row, col) || self.near_shallow(sh, row, col)
    }

    fn blocked(&self, sh: Option<&Shallow<'_>>, row: i64, col: i64) -> bool {
        self.land_cell(row, col)
            || sh.is_some_and(|sh| !sh.exempt((row, col)) && self.shallow_cell(sh, row, col))
    }

    /// True if the point lies in a land cell.
    #[must_use]
    pub fn is_land(&self, p: GeoPoint) -> bool {
        self.land_cell(self.row_of(p.lat_deg), self.col_of(p.lon_deg))
    }

    fn cell_centre(&self, row: i64, col: i64) -> GeoPoint {
        #[allow(clippy::cast_precision_loss)]
        GeoPoint::new(
            90.0 - (row as f64 + 0.5) / self.per_deg,
            -180.0 + (col as f64 + 0.5) / self.per_deg,
        )
    }

    /// True if the straight cell line from (r0,c0) to (r1,c1) (columns unwrapped) stays in water and
    /// away from the coast, except near its own two ends (so legs may start and end close inshore).
    fn line_clear(
        &self,
        sh: Option<&Shallow<'_>>,
        (r0, c0): (i64, i64),
        (r1, c1): (i64, i64),
    ) -> bool {
        let dr = r1 - r0;
        let dc = c1 - c0;
        let n = 4 * dr.abs().max(dc.abs()).max(1);
        for i in 0..=n {
            #[allow(clippy::cast_precision_loss)]
            let t = i as f64 / n as f64;
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let r = (r0 as f64 + 0.5 + t * dr as f64).floor() as i64;
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let c = (c0 as f64 + 0.5 + t * dc as f64).floor() as i64;
            if self.blocked(sh, r, c) {
                return false;
            }
            let near_end =
                (r - r0).abs().max((c - c0).abs()) <= 2 || (r - r1).abs().max((c - c1).abs()) <= 2;
            if !near_end && self.near_hazard(sh, r, c) {
                return false;
            }
        }
        true
    }

    /// Plans a sea route along `waypoints` starting at `from`. Each leg that would cross land is
    /// replaced by a detour; legs already clear are kept as given. Returns the new waypoint list
    /// (the given waypoints are all kept, in order) and how many legs were replanned.
    pub fn plan_route(
        &self,
        from: GeoPoint,
        waypoints: &[GeoPoint],
    ) -> Result<(Vec<GeoPoint>, usize), PlanError> {
        self.plan_route_for(from, waypoints, None)
            .map(|p| (p.waypoints, p.replanned))
    }

    /// As [`LandMask::plan_route`], also keeping out of water shallower than `limit`. A leg with
    /// no route deep enough is planned round land only and counted in `shallow_legs`.
    pub fn plan_route_for(
        &self,
        from: GeoPoint,
        waypoints: &[GeoPoint],
        limit: Option<DepthLimit<'_>>,
    ) -> Result<RoutePlan, PlanError> {
        if self.is_land(from) {
            return Err(PlanError::StartOnLand);
        }
        let mut plan = RoutePlan {
            waypoints: Vec::with_capacity(waypoints.len()),
            replanned: 0,
            shallow_legs: 0,
        };
        let mut at = from;
        for &wp in waypoints {
            if self.is_land(wp) {
                return Err(PlanError::WaypointOnLand);
            }
            let deep = limit.map(|l| self.plan_leg(at, wp, Some(l)));
            let leg = match deep {
                Some(Ok(leg)) => leg,
                Some(Err(_)) => {
                    plan.shallow_legs += 1;
                    self.plan_leg(at, wp, None)?
                }
                None => self.plan_leg(at, wp, None)?,
            };
            if leg.len() > 1 {
                plan.replanned += 1;
            }
            plan.waypoints.extend(leg);
            at = wp;
        }
        Ok(plan)
    }

    /// One leg: `[b]` when the straight line is clear, else intermediate turning points then `b`.
    fn plan_leg(
        &self,
        a: GeoPoint,
        b: GeoPoint,
        limit: Option<DepthLimit<'_>>,
    ) -> Result<Vec<GeoPoint>, PlanError> {
        let start = (self.row_of(a.lat_deg), self.col_of(a.lon_deg));
        let gr = self.row_of(b.lat_deg);
        let mut gc = self.col_of(b.lon_deg);
        // Take the shorter way round the antimeridian (columns may run past the grid edge).
        if gc - start.1 > self.cols / 2 {
            gc -= self.cols;
        } else if start.1 - gc > self.cols / 2 {
            gc += self.cols;
        }
        let goal = (gr, gc);
        let shallow = limit.map(|limit| Shallow {
            limit,
            start,
            goal,
            cache: RefCell::new(BTreeMap::new()),
        });
        let sh = shallow.as_ref();
        if self.line_clear(sh, start, goal) {
            return Ok(vec![b]);
        }
        let span = (goal.0 - start.0).abs().max((goal.1 - start.1).abs());
        let mut last_err = PlanError::NoSeaRoute;
        for margin in [span / 2 + 60, span * 2 + 180] {
            match self.astar(sh, start, goal, margin) {
                Ok(path) => {
                    let mut pts: Vec<GeoPoint> = self
                        .shorten(sh, &path)
                        .into_iter()
                        .map(|(r, c)| self.cell_centre(r, c))
                        .collect();
                    pts.pop(); // the goal cell: replaced by the exact waypoint
                    pts.push(b);
                    return Ok(pts);
                }
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    /// A* over water cells in a bounding region; returns the cell path start..=goal.
    fn astar(
        &self,
        sh: Option<&Shallow<'_>>,
        start: (i64, i64),
        goal: (i64, i64),
        margin: i64,
    ) -> Result<Vec<(i64, i64)>, PlanError> {
        const STEPS: [(i64, i64, u32); 8] = [
            (-1, 0, 10),
            (1, 0, 10),
            (0, -1, 10),
            (0, 1, 10),
            (-1, -1, 14),
            (-1, 1, 14),
            (1, -1, 14),
            (1, 1, 14),
        ];
        let r_lo = (start.0.min(goal.0) - margin).max(0);
        let r_hi = (start.0.max(goal.0) + margin).min(self.rows - 1);
        let c_lo = start.1.min(goal.1) - margin;
        let c_hi = start.1.max(goal.1) + margin;
        let (h, w) = (r_hi - r_lo + 1, c_hi - c_lo + 1);
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let n = (h * w) as usize;
        if n > MAX_REGION_CELLS {
            return Err(PlanError::TooFar);
        }
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let idx = |r: i64, c: i64| ((r - r_lo) * w + (c - c_lo)) as usize;
        let octile = |r: i64, c: i64| -> u32 {
            let dr = (r - goal.0).abs();
            let dc = (c - goal.1).abs();
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let v = (10 * dr.max(dc) + 4 * dr.min(dc)) as u32;
            v
        };
        let mut g = vec![u32::MAX; n];
        let mut came = vec![u32::MAX; n];
        let mut heap = BinaryHeap::new();
        g[idx(start.0, start.1)] = 0;
        #[allow(clippy::cast_possible_truncation)]
        heap.push(std::cmp::Reverse((
            octile(start.0, start.1),
            idx(start.0, start.1) as u32,
        )));
        while let Some(std::cmp::Reverse((_, cur))) = heap.pop() {
            let cur_us = cur as usize;
            let (r, c) = (i64::from(cur) / w + r_lo, i64::from(cur) % w + c_lo);
            if (r, c) == goal {
                let mut path = vec![(r, c)];
                let mut k = cur_us;
                while came[k] != u32::MAX {
                    k = came[k] as usize;
                    #[allow(clippy::cast_possible_wrap)]
                    path.push((k as i64 / w + r_lo, k as i64 % w + c_lo));
                }
                path.reverse();
                return Ok(path);
            }
            let g_cur = g[cur_us];
            for (dr, dc, base) in STEPS {
                let (nr, nc) = (r + dr, c + dc);
                if nr < r_lo || nr > r_hi || nc < c_lo || nc > c_hi || self.blocked(sh, nr, nc) {
                    continue;
                }
                // No corner-cutting between two land cells.
                if dr != 0 && dc != 0 && (self.land_cell(r + dr, c) || self.land_cell(r, c + dc)) {
                    continue;
                }
                let cost = if self.near_hazard(sh, nr, nc) {
                    base * COASTAL_PENALTY
                } else {
                    base
                };
                let ng = g_cur.saturating_add(cost);
                let ni = idx(nr, nc);
                if ng < g[ni] {
                    g[ni] = ng;
                    came[ni] = cur;
                    #[allow(clippy::cast_possible_truncation)]
                    heap.push(std::cmp::Reverse((
                        ng.saturating_add(octile(nr, nc)),
                        ni as u32,
                    )));
                }
            }
        }
        Err(PlanError::NoSeaRoute)
    }

    /// Greedy string-pulling: from each kept cell, jump to the farthest later cell in clear sight.
    fn shorten(&self, sh: Option<&Shallow<'_>>, path: &[(i64, i64)]) -> Vec<(i64, i64)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i + 1 < path.len() {
            let mut j = path.len() - 1;
            while j > i + 1 && !self.line_clear(sh, path[i], path[j]) {
                j -= 1;
            }
            out.push(path[j]);
            i = j;
        }
        out
    }

    /// A mask with no land at all (open ocean everywhere).
    #[must_use]
    pub fn empty(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            per_deg: 1.0,
            cols: 360,
            rows: 180,
            stride: 6,
            land: vec![0; 6 * 180],
            near_land: vec![0; 6 * 180],
        }
    }
}

/// The bundled Natural Earth 1:50m land mask, built once per process.
pub fn embedded() -> Arc<LandMask> {
    static MASK: OnceLock<Arc<LandMask>> = OnceLock::new();
    MASK.get_or_init(|| {
        let text = include_str!("../../../data/maps/ne_50m_land.geojson");
        match LandMask::from_geojson(text, EMBEDDED_CELLS_PER_DEGREE, EMBEDDED_MASK_ID) {
            Ok(m) => Arc::new(m),
            // The bundled file is fixed at build time and covered by tests, so this cannot fail at
            // runtime; an empty-land mask keeps the engine usable rather than aborting.
            Err(_) => Arc::new(LandMask::empty(EMBEDDED_MASK_ID)),
        }
    })
    .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inverse;

    fn mask() -> Arc<LandMask> {
        embedded()
    }

    #[test]
    fn known_land_and_sea_points() {
        let m = mask();
        for (lat, lon, land, what) in [
            (-33.0, 147.0, true, "inland New South Wales"),
            (-42.0, 146.5, true, "Tasmania"),
            (48.0, 10.0, true, "southern Germany"),
            (-39.3, 146.0, false, "Bass Strait"),
            (0.0, -150.0, false, "central Pacific"),
            (35.0, -40.0, false, "North Atlantic"),
            (39.0, 33.0, true, "central Anatolia"),
        ] {
            assert_eq!(m.is_land(GeoPoint::new(lat, lon)), land, "{what}");
        }
    }

    #[test]
    fn clear_leg_is_kept_as_given() {
        let m = mask();
        let a = GeoPoint::new(-39.2, 147.0);
        let b = GeoPoint::new(-39.4, 145.5);
        let (route, replanned) = m.plan_route(a, &[b]).unwrap();
        assert_eq!(route, vec![b]);
        assert_eq!(replanned, 0);
    }

    #[test]
    fn leg_across_tasmania_is_routed_round_it() {
        let m = mask();
        // West of Tasmania to east of it: the straight line crosses the island.
        let a = GeoPoint::new(-42.0, 144.0);
        let b = GeoPoint::new(-42.0, 149.5);
        let (route, replanned) = m.plan_route(a, &[b]).unwrap();
        assert_eq!(replanned, 1);
        assert!(route.len() >= 2, "{route:?}");
        assert_eq!(*route.last().unwrap(), b);
        // Every leg of the new route, densely sampled, stays at sea.
        let mut at = a;
        for &p in &route {
            let leg = inverse(at, p);
            for k in 0..=200 {
                let q = crate::direct(at, leg.azimuth1_deg, leg.distance_m * f64::from(k) / 200.0)
                    .point;
                assert!(!m.is_land(q), "leg {at:?} -> {p:?} touches land at {q:?}");
            }
            at = p;
        }
    }

    #[test]
    fn waypoints_on_land_and_starts_on_land_are_refused() {
        let m = mask();
        let sea = GeoPoint::new(-39.3, 146.0);
        assert_eq!(
            m.plan_route(sea, &[GeoPoint::new(-33.0, 147.0)]),
            Err(PlanError::WaypointOnLand)
        );
        assert_eq!(
            m.plan_route(GeoPoint::new(-33.0, 147.0), &[sea]),
            Err(PlanError::StartOnLand)
        );
    }

    #[test]
    fn antimeridian_crossing_in_open_ocean_is_direct() {
        let m = mask();
        let b = GeoPoint::new(0.0, -179.0);
        let (route, replanned) = m.plan_route(GeoPoint::new(0.0, 179.0), &[b]).unwrap();
        assert_eq!((route, replanned), (vec![b], 0));
    }

    #[test]
    fn engine_copy_matches_desktop_basemap() {
        let engine = include_bytes!("../../../data/maps/ne_50m_land.geojson");
        let desktop = include_bytes!("../../../apps/desktop/public/basemap/ne_50m_land.geojson");
        assert!(
            engine == desktop,
            "data/maps and the desktop basemap must be the same file"
        );
    }

    /// A world of 100 m water with a 5 m bank along the meridian (longitudes −1° to 1°), cut by
    /// a deep channel at 20–22°N when `channel` is set.
    fn bank(channel: bool) -> crate::terrain::Terrain {
        use crate::terrain::{encode, PackHeader, Terrain, TerrainPack};
        let (rows, cols) = (180u32, 360u32);
        let heights: Vec<i16> = (0..rows * cols)
            .map(|i| {
                let lat = -89.5 + f64::from(i / cols);
                let lon = -179.5 + f64::from(i % cols);
                let in_bank = lon.abs() < 1.0;
                let in_channel = channel && (20.0..=22.0).contains(&lat);
                if in_bank && !in_channel {
                    -5
                } else {
                    -100
                }
            })
            .collect();
        let header = PackHeader {
            name: "bank".into(),
            source: "test".into(),
            credit: "test".into(),
            lat0: -89 * 36_000 - 18_000,
            lon0: -179 * 36_000 - 18_000,
            cell: 36_000,
            rows,
            cols,
            min_m: -100,
            max_m: -5,
        };
        let pack = TerrainPack::decode(&encode(&header, &heights).unwrap()).unwrap();
        Terrain::new(vec![Arc::new(pack)]).unwrap()
    }

    #[test]
    fn deep_draft_routes_round_a_shallow_bank() {
        let mask = LandMask::empty("open");
        let terrain = bank(true);
        let (from, to) = (GeoPoint::new(0.0, -5.0), GeoPoint::new(0.0, 5.0));
        let deep = DepthLimit {
            terrain: &terrain,
            min_depth_m: 8.0,
        };
        let plan = mask.plan_route_for(from, &[to], Some(deep)).unwrap();
        assert_eq!(plan.shallow_legs, 0);
        assert_eq!(plan.replanned, 1);
        assert_eq!(plan.waypoints.last(), Some(&to));
        // It crosses the meridian only through the channel, and stays out of the bank.
        let mut at = from;
        for &wp in &plan.waypoints {
            for k in 0..=200 {
                let t = f64::from(k) / 200.0;
                let p = GeoPoint::new(
                    at.lat_deg + t * (wp.lat_deg - at.lat_deg),
                    at.lon_deg + t * (wp.lon_deg - at.lon_deg),
                );
                if p.lon_deg.abs() < 1.0 {
                    assert!(
                        (19.0..=23.0).contains(&p.lat_deg),
                        "crossed the bank at {p:?}"
                    );
                }
            }
            at = wp;
        }
        // A shallow draft goes straight across, as does a route planned without a limit.
        let shallow = DepthLimit {
            terrain: &terrain,
            min_depth_m: 3.0,
        };
        let plan = mask.plan_route_for(from, &[to], Some(shallow)).unwrap();
        assert_eq!((plan.waypoints.as_slice(), plan.replanned), (&[to][..], 0));
        assert_eq!(mask.plan_route(from, &[to]).unwrap(), (vec![to], 0));
    }

    #[test]
    fn no_deep_route_falls_back_and_says_so() {
        let mask = LandMask::empty("open");
        let terrain = bank(false);
        let (from, to) = (GeoPoint::new(0.0, -5.0), GeoPoint::new(0.0, 5.0));
        let limit = DepthLimit {
            terrain: &terrain,
            min_depth_m: 8.0,
        };
        let plan = mask.plan_route_for(from, &[to], Some(limit)).unwrap();
        assert_eq!(plan.shallow_legs, 1);
        assert_eq!(plan.waypoints, vec![to]);
    }

    #[test]
    fn a_leg_may_start_and_end_in_shallow_water() {
        // Inside the bank at both ends, 1° apart along it: within 2 cells of the ends.
        let mask = LandMask::empty("open");
        let terrain = bank(false);
        let limit = DepthLimit {
            terrain: &terrain,
            min_depth_m: 8.0,
        };
        let (from, to) = (GeoPoint::new(0.0, 0.0), GeoPoint::new(1.5, 0.0));
        let plan = mask.plan_route_for(from, &[to], Some(limit)).unwrap();
        assert_eq!((plan.waypoints, plan.shallow_legs), (vec![to], 0));
    }
}
