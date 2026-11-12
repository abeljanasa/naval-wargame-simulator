//! `nws-batch terrain`: builds terrain packs from GEBCO netCDF downloads (Phase 4 slice 2).
//!
//! The NOAA ERDDAP server delivers GEBCO subsets as netCDF-3 classic files. That format is a short
//! big-endian header followed by the arrays, so it is read here directly rather than through a C
//! library. Only what GEBCO uses is supported: fixed-size (non-record) variables of type byte,
//! short, int, float or double.

use anyhow::{bail, ensure, Context, Result};
use sim_geo::terrain::{self, PackHeader, TerrainPack, UNITS_PER_DEGREE};
use sim_geo::GeoPoint;
use std::collections::BTreeMap;
use std::path::Path;

/// Credit the GEBCO licence asks for (docs/db/data-dictionary.md).
pub const GEBCO_2020_CREDIT: &str =
    "GEBCO Compilation Group (2020) GEBCO 2020 Grid (doi:10.5285/a29c5465-b138-234d-e053-6c86abc040b9)";
pub const GEBCO_2020_SOURCE: &str = "GEBCO 2020 Grid, 15 arc-seconds, via NOAA CoastWatch ERDDAP";

/// One variable of a netCDF-3 file.
#[derive(Debug, Clone)]
pub struct NcVar {
    pub shape: Vec<usize>,
    nc_type: u32,
    begin: u64,
}

/// A netCDF-3 classic file held in memory.
pub struct NetCdf {
    bytes: Vec<u8>,
    pub vars: BTreeMap<String, NcVar>,
}

struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.b.len());
        let end = end.context("netCDF header is truncated")?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into()?))
    }
    fn padded(&mut self, n: usize) -> Result<&[u8]> {
        let pad = (4 - n % 4) % 4;
        let s = self.take(n + pad)?;
        Ok(&s[..n])
    }
    fn name(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        Ok(String::from_utf8_lossy(self.padded(n)?).into_owned())
    }
    fn skip_attributes(&mut self) -> Result<()> {
        let tag = self.u32()?;
        let n = self.u32()?;
        ensure!(tag == 0x0C || (tag == 0 && n == 0), "bad attribute list");
        for _ in 0..n {
            self.name()?;
            let t = self.u32()?;
            let count = self.u32()? as usize;
            self.padded(count * type_size(t)?)?;
        }
        Ok(())
    }
}

fn type_size(nc_type: u32) -> Result<usize> {
    Ok(match nc_type {
        1 | 2 => 1,
        3 => 2,
        4 | 5 => 4,
        6 => 8,
        t => bail!("netCDF type {t} is not supported"),
    })
}

impl NetCdf {
    pub fn open(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(bytes)
    }

    pub fn parse(bytes: Vec<u8>) -> Result<Self> {
        let mut c = Cursor { b: &bytes, at: 0 };
        let magic = c.take(4)?;
        ensure!(&magic[..3] == b"CDF", "not a netCDF-3 file");
        let wide = match magic[3] {
            1 => false,
            2 => true,
            v => bail!("netCDF-3 variant {v} is not supported (netCDF-4/HDF5 files are not)"),
        };
        let _numrecs = c.u32()?;
        // Dimensions.
        let tag = c.u32()?;
        let n = c.u32()?;
        ensure!(tag == 0x0A || (tag == 0 && n == 0), "bad dimension list");
        let mut dims = Vec::new();
        for _ in 0..n {
            c.name()?;
            let len = c.u32()? as usize;
            ensure!(len > 0, "record (unlimited) variables are not supported");
            dims.push(len);
        }
        c.skip_attributes()?;
        // Variables.
        let tag = c.u32()?;
        let n = c.u32()?;
        ensure!(tag == 0x0B || (tag == 0 && n == 0), "bad variable list");
        let mut vars = BTreeMap::new();
        for _ in 0..n {
            let name = c.name()?;
            let rank = c.u32()? as usize;
            let mut shape = Vec::with_capacity(rank);
            for _ in 0..rank {
                let d = c.u32()? as usize;
                shape.push(*dims.get(d).context("bad dimension id")?);
            }
            c.skip_attributes()?;
            let nc_type = c.u32()?;
            type_size(nc_type)?;
            let _vsize = c.u32()?;
            let begin = if wide { c.u64()? } else { u64::from(c.u32()?) };
            vars.insert(
                name,
                NcVar {
                    shape,
                    nc_type,
                    begin,
                },
            );
        }
        let me = Self { bytes, vars };
        for (name, v) in &me.vars {
            let len = v.shape.iter().product::<usize>() * type_size(v.nc_type)?;
            ensure!(
                usize::try_from(v.begin)
                    .ok()
                    .and_then(|b| b.checked_add(len))
                    .is_some_and(|e| e <= me.bytes.len()),
                "variable {name} runs past the end of the file"
            );
        }
        Ok(me)
    }

    fn var(&self, name: &str) -> Result<(&NcVar, &[u8])> {
        let v = self
            .vars
            .get(name)
            .with_context(|| format!("no variable {name:?}"))?;
        let start = usize::try_from(v.begin)?;
        let len = v.shape.iter().product::<usize>() * type_size(v.nc_type)?;
        Ok((v, &self.bytes[start..start + len]))
    }

    /// A variable as `f64`s.
    pub fn f64s(&self, name: &str) -> Result<Vec<f64>> {
        let (v, b) = self.var(name)?;
        Ok(match v.nc_type {
            1 => b.iter().map(|&x| f64::from(x.cast_signed())).collect(),
            3 => b
                .as_chunks::<2>()
                .0
                .iter()
                .map(|x| f64::from(i16::from_be_bytes(*x)))
                .collect(),
            4 => b
                .as_chunks::<4>()
                .0
                .iter()
                .map(|x| f64::from(i32::from_be_bytes(*x)))
                .collect(),
            5 => b
                .as_chunks::<4>()
                .0
                .iter()
                .map(|x| f64::from(f32::from_be_bytes(*x)))
                .collect(),
            6 => b
                .as_chunks::<8>()
                .0
                .iter()
                .map(|x| f64::from_be_bytes(*x))
                .collect(),
            t => bail!("variable {name} has type {t}, not a number"),
        })
    }

    /// A `short` variable.
    pub fn i16s(&self, name: &str) -> Result<Vec<i16>> {
        let (v, b) = self.var(name)?;
        ensure!(v.nc_type == 3, "variable {name} is not of type short");
        Ok(b.as_chunks::<2>()
            .0
            .iter()
            .map(|x| i16::from_be_bytes(*x))
            .collect())
    }
}

/// A regular axis in grid units: first cell centre and spacing.
fn axis(values: &[f64], what: &str) -> Result<(i64, i64)> {
    ensure!(values.len() >= 2, "{what}: fewer than two values");
    let u = UNITS_PER_DEGREE as f64;
    let first = values[0] * u;
    let step = (values[values.len() - 1] - values[0]) * u / (values.len() - 1) as f64;
    let (first_i, step_i) = (first.round() as i64, step.round() as i64);
    ensure!(step_i > 0, "{what} must increase");
    ensure!(
        (first - first_i as f64).abs() < 0.05 && (step - step_i as f64).abs() < 0.05,
        "{what} is not on a whole tenth of an arc-second ({first} + n × {step})"
    );
    for (i, v) in values.iter().enumerate() {
        let expected = first_i + i as i64 * step_i;
        ensure!(
            (v * u - expected as f64).abs() < 0.5,
            "{what} is not evenly spaced at index {i}"
        );
    }
    Ok((first_i, step_i))
}

/// Builds a pack from a GEBCO download: `elevation[latitude][longitude]`, both axes ascending.
/// `average` > 1 averages `average × average` blocks into one cell (coarser, smaller pack).
pub fn build(input: &Path, name: &str, average: u32) -> Result<(PackHeader, Vec<u8>)> {
    ensure!(average >= 1, "--average must be at least 1");
    let nc = NetCdf::open(input)?;
    let lat = nc.f64s("latitude")?;
    let lon = nc.f64s("longitude")?;
    let (lat0, dlat) = axis(&lat, "latitude")?;
    let (lon0, dlon) = axis(&lon, "longitude")?;
    ensure!(
        dlat == dlon,
        "latitude and longitude spacing differ ({dlat} and {dlon} tenths of an arc-second)"
    );
    let elev = nc.i16s("elevation")?;
    let (v, _) = nc.var("elevation")?;
    ensure!(
        v.shape == [lat.len(), lon.len()],
        "elevation must be [latitude][longitude], found {:?}",
        v.shape
    );
    let (rows, cols) = (lat.len(), lon.len());
    let a = average as usize;
    let (out_rows, out_cols) = (rows / a, cols / a);
    ensure!(
        out_rows > 0 && out_cols > 0,
        "--average is larger than the grid"
    );
    let offset = i64::from(average - 1) * dlat;
    ensure!(
        offset % 2 == 0,
        "averaging {average} cells would put the centres off the grid"
    );
    let n = i64::from(average * average);
    let mut heights = Vec::with_capacity(out_rows * out_cols);
    for r in 0..out_rows {
        for c in 0..out_cols {
            let mut sum = 0i64;
            for rr in 0..a {
                let row = &elev[(r * a + rr) * cols..];
                for cc in 0..a {
                    sum += i64::from(row[c * a + cc]);
                }
            }
            // Mean rounded half up, in integers.
            heights.push(i16::try_from((2 * sum + n).div_euclid(2 * n))?);
        }
    }
    let to_i32 = |x: i64| i32::try_from(x).context("grid position out of range");
    let mut lon0 = lon0 + offset / 2;
    if lon0 >= 180 * UNITS_PER_DEGREE {
        lon0 -= 360 * UNITS_PER_DEGREE;
    }
    let header = PackHeader {
        name: name.to_owned(),
        source: GEBCO_2020_SOURCE.to_owned(),
        credit: GEBCO_2020_CREDIT.to_owned(),
        lat0: to_i32(lat0 + offset / 2)?,
        lon0: to_i32(lon0)?,
        cell: u32::try_from(dlat * i64::from(average))?,
        rows: u32::try_from(out_rows)?,
        cols: u32::try_from(out_cols)?,
        min_m: heights.iter().copied().min().unwrap_or(0),
        max_m: heights.iter().copied().max().unwrap_or(0),
    };
    let bytes = terrain::encode(&header, &heights)?;
    Ok((header, bytes))
}

/// Writes the pack and a `sha256sum`-style sidecar next to it. Returns the pack's SHA-256.
pub fn write(out: &Path, bytes: &[u8]) -> Result<String> {
    let sha = terrain::sha256_hex(bytes);
    std::fs::write(out, bytes).with_context(|| format!("writing {}", out.display()))?;
    let file = out
        .file_name()
        .context("output has no file name")?
        .to_string_lossy();
    let sidecar = out.with_file_name(format!("{file}.sha256"));
    std::fs::write(&sidecar, format!("{sha}  {file}\n"))
        .with_context(|| format!("writing {}", sidecar.display()))?;
    Ok(sha)
}

/// A summary of a pack, for `terrain info`.
pub fn info(path: &Path) -> Result<serde_json::Value> {
    let pack = TerrainPack::load(path).with_context(|| format!("reading {}", path.display()))?;
    let h = pack.header();
    Ok(serde_json::json!({
        "id": pack.id(),
        "sha256": pack.sha256(),
        "name": h.name,
        "source": h.source,
        "credit": h.credit,
        "cell_arcsec": h.cell_arcsec(),
        "rows": h.rows,
        "cols": h.cols,
        "wraps": h.wraps(),
        "bounds_deg": h.bounds_deg(),
        "min_m": h.min_m,
        "max_m": h.max_m,
        "bytes": std::fs::metadata(path)?.len(),
    }))
}

/// Height at a point from one or more packs (finest first), for `terrain sample`.
pub fn sample(packs: &[std::path::PathBuf], lat: f64, lon: f64) -> Result<serde_json::Value> {
    let loaded = packs
        .iter()
        .map(|p| {
            TerrainPack::load(p)
                .map(std::sync::Arc::new)
                .with_context(|| format!("reading {}", p.display()))
        })
        .collect::<Result<Vec<_>>>()?;
    let t = sim_geo::terrain::Terrain::new(loaded).context("no packs")?;
    let p = GeoPoint::new(lat, lon);
    Ok(serde_json::json!({
        "terrain": t.id(),
        "lat_deg": lat,
        "lon_deg": lon,
        "height_m": t.height_m(p),
        "depth_m": t.depth_m(p),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal netCDF-3 classic file shaped like an ERDDAP GEBCO subset.
    fn tiny_nc(lat: &[f64], lon: &[f64], elev: &[i16]) -> Vec<u8> {
        fn name(b: &mut Vec<u8>, s: &str) {
            b.extend_from_slice(&(s.len() as u32).to_be_bytes());
            b.extend_from_slice(s.as_bytes());
            while !b.len().is_multiple_of(4) {
                b.push(0);
            }
        }
        let mut b = b"CDF\x01".to_vec();
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&0x0Au32.to_be_bytes());
        b.extend_from_slice(&2u32.to_be_bytes());
        name(&mut b, "latitude");
        b.extend_from_slice(&(lat.len() as u32).to_be_bytes());
        name(&mut b, "longitude");
        b.extend_from_slice(&(lon.len() as u32).to_be_bytes());
        // One global attribute, to exercise skipping.
        b.extend_from_slice(&0x0Cu32.to_be_bytes());
        b.extend_from_slice(&1u32.to_be_bytes());
        name(&mut b, "title");
        b.extend_from_slice(&2u32.to_be_bytes());
        b.extend_from_slice(&5u32.to_be_bytes());
        b.extend_from_slice(b"GEBCO\0\0\0");
        b.extend_from_slice(&0x0Bu32.to_be_bytes());
        b.extend_from_slice(&3u32.to_be_bytes());
        let elev_bytes = elev.len() * 2;
        let sizes = [lat.len() * 8, lon.len() * 8, elev_bytes.div_ceil(4) * 4];
        let vars: [(&str, &[u32], u32); 3] = [
            ("latitude", &[0], 6),
            ("longitude", &[1], 6),
            ("elevation", &[0, 1], 3),
        ];
        let mut header_rest = Vec::new();
        let mut begins_at = Vec::new();
        for (n, dims, t) in vars {
            name(&mut header_rest, n);
            header_rest.extend_from_slice(&(dims.len() as u32).to_be_bytes());
            for d in dims {
                header_rest.extend_from_slice(&d.to_be_bytes());
            }
            header_rest.extend_from_slice(&[0; 8]);
            header_rest.extend_from_slice(&t.to_be_bytes());
            header_rest.extend_from_slice(&0u32.to_be_bytes());
            begins_at.push(header_rest.len());
            header_rest.extend_from_slice(&0u32.to_be_bytes());
        }
        let data_start = b.len() + header_rest.len();
        let mut begin = data_start;
        for (i, at) in begins_at.iter().enumerate() {
            header_rest[*at..*at + 4].copy_from_slice(&(begin as u32).to_be_bytes());
            begin += sizes[i];
        }
        b.extend_from_slice(&header_rest);
        for v in lat.iter().chain(lon) {
            b.extend_from_slice(&v.to_be_bytes());
        }
        for v in elev {
            b.extend_from_slice(&v.to_be_bytes());
        }
        while !b.len().is_multiple_of(4) {
            b.push(0);
        }
        b
    }

    #[test]
    fn reads_a_netcdf3_grid_and_builds_a_pack() {
        let step = 15.0 / 3600.0;
        let lat: Vec<f64> = (0..4)
            .map(|i| 10.0 + step / 2.0 + f64::from(i) * step)
            .collect();
        let lon: Vec<f64> = (0..4)
            .map(|i| 120.0 + step / 2.0 + f64::from(i) * step)
            .collect();
        let elev: Vec<i16> = (0..16).map(|i| -100 + i * 10).collect();
        let dir = std::env::temp_dir().join(format!("nws-terrain-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("tiny.nc");
        std::fs::write(&input, tiny_nc(&lat, &lon, &elev)).unwrap();

        let (h, bytes) = build(&input, "tiny", 1).unwrap();
        assert_eq!((h.rows, h.cols, h.cell), (4, 4, 150));
        assert_eq!((h.lat0, h.lon0), (10 * 36_000 + 75, 120 * 36_000 + 75));
        assert_eq!((h.min_m, h.max_m), (-100, 50));
        let pack = TerrainPack::decode(&bytes).unwrap();
        // The first cell centre is the first value; one row north adds 40 m.
        assert_eq!(pack.height_m(GeoPoint::new(lat[0], lon[0])), Some(-100.0));
        assert_eq!(pack.height_m(GeoPoint::new(lat[1], lon[0])), Some(-60.0));

        // 2 × 2 averaging: centres move to the middle of each block.
        let (h2, bytes2) = build(&input, "tiny-avg", 2).unwrap();
        assert_eq!((h2.rows, h2.cols, h2.cell), (2, 2, 300));
        assert_eq!(h2.lat0, 10 * 36_000 + 150);
        let pack2 = TerrainPack::decode(&bytes2).unwrap();
        // Block (−100, −90, −60, −50) → −75.
        assert_eq!(
            pack2.height_m(GeoPoint::new(10.0 + step, 120.0 + step)),
            Some(-75.0)
        );

        let out = dir.join("tiny.nwsterrain");
        let sha = write(&out, &bytes).unwrap();
        let sidecar = std::fs::read_to_string(dir.join("tiny.nwsterrain.sha256")).unwrap();
        assert_eq!(sidecar, format!("{sha}  tiny.nwsterrain\n"));
        assert_eq!(info(&out).unwrap()["id"], pack.id());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn refuses_uneven_axes_and_other_formats() {
        assert!(NetCdf::parse(b"\x89HDF\r\n\x1a\n".to_vec()).is_err());
        assert!(axis(&[0.0, 1.0, 3.0], "latitude").is_err());
        assert!(axis(&[1.0, 0.0], "latitude").is_err());
    }
}
