//! Offline imagery tile packs (Phase 4 slice 6, `docs/models/imagery.md`).
//!
//! The client's licensed imagery arrives as a tile pack: **MBTiles** (SQLite) or **PMTiles** v3.
//! Packs are installed into the app's `imagery` folder and served to the map from there through
//! the app's own `nwstiles` scheme: nothing is fetched from a network. Raster tiles only (PNG,
//! JPEG, WebP). Each pack's attribution, from its metadata, is shown with the map.

use flate2::read::GzDecoder;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Largest pack the app installs through the window (bigger packs are copied into the folder).
pub const MAX_INSTALL_BYTES: usize = 1 << 30;

/// One installed pack, as the UI sees it.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ImageryInfo {
    /// File name in the imagery folder; also the pack's key in tile URLs.
    pub file: String,
    pub name: String,
    /// `png`, `jpg` or `webp`.
    pub format: String,
    pub min_zoom: u8,
    pub max_zoom: u8,
    /// West, south, east, north, degrees (whole world if the pack does not say).
    pub bounds: [f64; 4],
    /// The credit the imagery's licence asks for (`None`: the pack does not give one).
    pub attribution: Option<String>,
    /// Tile size in pixels (256 unless the pack says otherwise).
    pub tile_size: u32,
}

/// The imagery packs on this machine.
#[derive(Debug, Clone, Default, Serialize, specta::Type)]
pub struct ImageryStatus {
    pub packs: Vec<ImageryInfo>,
    /// Files in the folder that could not be read, and why.
    pub problems: Vec<String>,
    pub folder: String,
}

enum Source {
    Mb(Mutex<rusqlite::Connection>),
    Pm(Mutex<std::fs::File>, PmHeader),
}

pub struct Pack {
    info: ImageryInfo,
    source: Source,
}

/// The installed packs, by file name.
#[derive(Default)]
pub struct Imagery {
    pub folder: Option<PathBuf>,
    packs: BTreeMap<String, Pack>,
    problems: Vec<String>,
}

impl Imagery {
    #[must_use]
    pub fn status(&self) -> ImageryStatus {
        ImageryStatus {
            packs: self.packs.values().map(|p| p.info.clone()).collect(),
            problems: self.problems.clone(),
            folder: self
                .folder
                .as_ref()
                .map(|f| f.display().to_string())
                .unwrap_or_default(),
        }
    }

    /// Reads every `.mbtiles` and `.pmtiles` file in the folder.
    pub fn rescan(&mut self) {
        self.packs.clear();
        self.problems.clear();
        let Some(dir) = self.folder.clone() else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut files: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
        files.sort();
        for f in files {
            let ext = f
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase());
            let name = f
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let opened = match ext.as_deref() {
                Some("mbtiles") => open_mbtiles(&f, &name),
                Some("pmtiles") => open_pmtiles(&f, &name),
                _ => continue,
            };
            match opened {
                Ok(p) => {
                    self.packs.insert(name, p);
                }
                Err(e) => self.problems.push(format!("{name}: {e}")),
            }
        }
    }

    /// One tile: its bytes and content type. `None`: no such pack or tile.
    #[must_use]
    pub fn tile(&self, file: &str, z: u8, x: u32, y: u32) -> Option<(Vec<u8>, &'static str)> {
        let p = self.packs.get(file)?;
        let mime = match p.info.format.as_str() {
            "jpg" => "image/jpeg",
            "webp" => "image/webp",
            _ => "image/png",
        };
        let bytes = match &p.source {
            Source::Mb(c) => mb_tile(&*c.lock().ok()?, z, x, y)?,
            Source::Pm(f, h) => pm_tile(&mut *f.lock().ok()?, h, z, x, y)?,
        };
        Some((bytes, mime))
    }
}

/// Installs a pack the user picked into the folder (the name is kept, made safe).
pub fn install(folder: &Path, file_name: &str, bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > MAX_INSTALL_BYTES {
        return Err(format!(
            "the pack is {} MB; packs over {} MB are copied into the imagery folder instead",
            bytes.len() >> 20,
            MAX_INSTALL_BYTES >> 20
        ));
    }
    let safe: String = Path::new(file_name)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_. ".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let lower = safe.to_ascii_lowercase();
    if !(lower.ends_with(".mbtiles") || lower.ends_with(".pmtiles")) {
        return Err("choose an .mbtiles or .pmtiles tile pack".into());
    }
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let path = folder.join(&safe);
    let tmp = folder.join(format!("{safe}.part"));
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    // Check it opens before it replaces anything.
    let check = if lower.ends_with(".mbtiles") {
        open_mbtiles(&tmp, &safe).map(|_| ())
    } else {
        open_pmtiles(&tmp, &safe).map(|_| ())
    };
    if let Err(e) = check {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{safe} is not a usable tile pack: {e}"));
    }
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(safe)
}

fn format_of(s: &str) -> Result<String, String> {
    match s.to_ascii_lowercase().as_str() {
        "png" => Ok("png".into()),
        "jpg" | "jpeg" => Ok("jpg".into()),
        "webp" => Ok("webp".into()),
        "pbf" | "mvt" => Err("vector tiles: only imagery (raster) packs are supported".into()),
        other => Err(format!("tile format {other:?} is not supported")),
    }
}

// ---- MBTiles ----

fn open_mbtiles(path: &Path, file: &str) -> Result<Pack, String> {
    let c = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())?;
    let mut meta = BTreeMap::new();
    {
        let mut st = c
            .prepare("SELECT name, value FROM metadata")
            .map_err(|_| "no metadata table: not an MBTiles file".to_owned())?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        for (k, v) in rows.flatten() {
            meta.insert(k, v);
        }
    }
    let format = format_of(meta.get("format").map_or("png", String::as_str))?;
    let (min_zoom, max_zoom): (u8, u8) = match (meta.get("minzoom"), meta.get("maxzoom")) {
        (Some(a), Some(b)) => (a.parse().unwrap_or(0), b.parse().unwrap_or(22)),
        _ => c
            .query_row(
                "SELECT MIN(zoom_level), MAX(zoom_level) FROM tiles",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .map(|(a, b)| (u8::try_from(a).unwrap_or(0), u8::try_from(b).unwrap_or(22)))
            .map_err(|_| "no tiles table: not an MBTiles file".to_owned())?,
    };
    let bounds = meta
        .get("bounds")
        .and_then(|b| {
            let v: Vec<f64> = b.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
        })
        .unwrap_or([-180.0, -85.05, 180.0, 85.05]);
    Ok(Pack {
        info: ImageryInfo {
            file: file.to_owned(),
            name: meta.get("name").cloned().unwrap_or_else(|| file.to_owned()),
            format,
            min_zoom,
            max_zoom,
            bounds,
            attribution: meta
                .get("attribution")
                .cloned()
                .filter(|a| !a.trim().is_empty()),
            tile_size: meta
                .get("tilesize")
                .and_then(|t| t.parse().ok())
                .unwrap_or(256),
        },
        source: Source::Mb(Mutex::new(c)),
    })
}

fn mb_tile(c: &rusqlite::Connection, z: u8, x: u32, y: u32) -> Option<Vec<u8>> {
    // MBTiles rows count from the south (TMS).
    let tms_y = (1u32 << z).checked_sub(1)?.checked_sub(y)?;
    c.query_row(
        "SELECT tile_data FROM tiles WHERE zoom_level = ?1 AND tile_column = ?2 AND tile_row = ?3",
        rusqlite::params![z, x, tms_y],
        |r| r.get::<_, Vec<u8>>(0),
    )
    .ok()
}

// ---- PMTiles v3 ----

#[derive(Debug, Clone, Copy)]
struct PmHeader {
    root_offset: u64,
    root_length: u64,
    leaf_offset: u64,
    data_offset: u64,
    internal_gzip: bool,
    tile_gzip: bool,
}

fn le_u64(b: &[u8], at: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(a)
}

fn le_i32(b: &[u8], at: usize) -> i32 {
    let mut a = [0u8; 4];
    a.copy_from_slice(&b[at..at + 4]);
    i32::from_le_bytes(a)
}

fn read_at(f: &mut std::fs::File, offset: u64, len: u64) -> Result<Vec<u8>, String> {
    if len > 64 << 20 {
        return Err("directory or tile larger than 64 MB".into());
    }
    let mut buf = vec![0u8; usize::try_from(len).map_err(|e| e.to_string())?];
    f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    f.read_exact(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

fn gunzip(b: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    GzDecoder::new(b)
        .take(256 << 20)
        .read_to_end(&mut out)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

fn open_pmtiles(path: &Path, file: &str) -> Result<Pack, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let h = read_at(&mut f, 0, 127)?;
    if &h[..7] != b"PMTiles" || h[7] != 3 {
        return Err("not a PMTiles version 3 file".into());
    }
    let compression = |c: u8, what: &str| match c {
        0 | 1 => Ok(false),
        2 => Ok(true),
        _ => Err(format!(
            "{what} compression {c} is not supported (none or gzip)"
        )),
    };
    let header = PmHeader {
        root_offset: le_u64(&h, 8),
        root_length: le_u64(&h, 16),
        leaf_offset: le_u64(&h, 40),
        data_offset: le_u64(&h, 56),
        internal_gzip: compression(h[97], "directory")?,
        tile_gzip: compression(h[98], "tile")?,
    };
    let format = match h[99] {
        2 => "png",
        3 => "jpg",
        4 => "webp",
        1 => return Err("vector tiles: only imagery (raster) packs are supported".into()),
        t => return Err(format!("tile type {t} is not supported")),
    }
    .to_owned();
    let e7 = |at: usize| f64::from(le_i32(&h, at)) / 1e7;
    let (meta_offset, meta_length) = (le_u64(&h, 24), le_u64(&h, 32));
    let mut meta_bytes = read_at(&mut f, meta_offset, meta_length)?;
    if header.internal_gzip {
        meta_bytes = gunzip(&meta_bytes)?;
    }
    let meta: serde_json::Value = serde_json::from_slice(&meta_bytes).unwrap_or_default();
    let text = |k: &str| {
        meta.get(k)
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .filter(|s| !s.trim().is_empty())
    };
    Ok(Pack {
        info: ImageryInfo {
            file: file.to_owned(),
            name: text("name").unwrap_or_else(|| file.to_owned()),
            format,
            min_zoom: h[100],
            max_zoom: h[101],
            bounds: [e7(102), e7(106), e7(110), e7(114)],
            attribution: text("attribution"),
            tile_size: meta
                .get("tilesize")
                .and_then(serde_json::Value::as_u64)
                .and_then(|t| u32::try_from(t).ok())
                .unwrap_or(256),
        },
        source: Source::Pm(Mutex::new(f), header),
    })
}

fn varint(b: &[u8], at: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    let mut shift = 0;
    loop {
        let byte = *b.get(*at)?;
        *at += 1;
        v |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

/// (tile id, offset, length, run length) per entry.
fn directory(b: &[u8]) -> Option<Vec<(u64, u64, u64, u64)>> {
    let mut at = 0;
    let n = usize::try_from(varint(b, &mut at)?).ok()?;
    if n > 10_000_000 {
        return None;
    }
    let mut e = vec![(0u64, 0u64, 0u64, 0u64); n];
    let mut last = 0u64;
    for x in &mut e {
        last = last.checked_add(varint(b, &mut at)?)?;
        x.0 = last;
    }
    for x in &mut e {
        x.3 = varint(b, &mut at)?;
    }
    for x in &mut e {
        x.2 = varint(b, &mut at)?;
    }
    for i in 0..n {
        let v = varint(b, &mut at)?;
        e[i].1 = if v == 0 && i > 0 {
            e[i - 1].1 + e[i - 1].2
        } else {
            v.checked_sub(1)?
        };
    }
    Some(e)
}

/// The PMTiles tile id of z/x/y: tiles of lower zooms first, then the Hilbert curve.
#[must_use]
pub fn tile_id(z: u8, x: u32, y: u32) -> u64 {
    let base: u64 = (0..z).map(|i| 1u64 << (2 * u32::from(i))).sum();
    let n = 1u64 << z;
    let (mut x, mut y) = (u64::from(x), u64::from(y));
    let mut d = 0u64;
    let mut s = n / 2;
    while s > 0 {
        let rx = u64::from(x & s > 0);
        let ry = u64::from(y & s > 0);
        d += s * s * ((3 * rx) ^ ry);
        // Rotate.
        if ry == 0 {
            if rx == 1 {
                x = n - 1 - x;
                y = n - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        s /= 2;
    }
    base + d
}

fn pm_tile(f: &mut std::fs::File, h: &PmHeader, z: u8, x: u32, y: u32) -> Option<Vec<u8>> {
    let id = tile_id(z, x, y);
    let (mut offset, mut length) = (h.root_offset, h.root_length);
    for _ in 0..4 {
        let mut raw = read_at(f, offset, length).ok()?;
        if h.internal_gzip {
            raw = gunzip(&raw).ok()?;
        }
        let dir = directory(&raw)?;
        let i = dir.partition_point(|e| e.0 <= id).checked_sub(1)?;
        let (eid, eoff, elen, run) = dir[i];
        if run == 0 {
            // A leaf directory.
            offset = h.leaf_offset + eoff;
            length = elen;
            continue;
        }
        if id >= eid + run {
            return None;
        }
        let bytes = read_at(f, h.data_offset + eoff, elen).ok()?;
        return if h.tile_gzip {
            gunzip(&bytes).ok()
        } else {
            Some(bytes)
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nws-imagery-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn hilbert_tile_ids_match_the_pmtiles_spec() {
        // Values from the PMTiles v3 specification's examples.
        assert_eq!(tile_id(0, 0, 0), 0);
        assert_eq!(tile_id(1, 0, 0), 1);
        assert_eq!(tile_id(1, 0, 1), 2);
        assert_eq!(tile_id(1, 1, 1), 3);
        assert_eq!(tile_id(1, 1, 0), 4);
        assert_eq!(tile_id(2, 0, 0), 5);
        assert_eq!(tile_id(12, 3423, 1763), 19_078_479);
    }

    #[test]
    fn serves_an_mbtiles_pack_with_its_attribution() {
        let dir = tmp("mb");
        let path = dir.join("coast.mbtiles");
        {
            let c = rusqlite::Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE metadata (name TEXT, value TEXT);
                 CREATE TABLE tiles (zoom_level INTEGER, tile_column INTEGER, tile_row INTEGER,
                                     tile_data BLOB);
                 INSERT INTO metadata VALUES ('name', 'Coast imagery'), ('format', 'jpg'),
                   ('attribution', '© Example Imagery Ltd, licensed to the client'),
                   ('minzoom', '1'), ('maxzoom', '2'), ('bounds', '119,11,122,14');",
            )
            .unwrap();
            // z1 x0 y0 (north-west) is TMS row 1.
            c.execute(
                "INSERT INTO tiles VALUES (1, 0, 1, ?1)",
                rusqlite::params![b"JPEGBYTES".to_vec()],
            )
            .unwrap();
        }
        let mut im = Imagery {
            folder: Some(dir.clone()),
            ..Imagery::default()
        };
        im.rescan();
        let s = im.status();
        assert_eq!(s.packs.len(), 1, "{:?}", s.problems);
        let p = &s.packs[0];
        assert_eq!(
            (p.name.as_str(), p.format.as_str(), p.min_zoom, p.max_zoom),
            ("Coast imagery", "jpg", 1, 2)
        );
        assert_eq!(p.bounds, [119.0, 11.0, 122.0, 14.0]);
        assert!(p
            .attribution
            .as_deref()
            .unwrap()
            .contains("Example Imagery"));
        assert_eq!(
            im.tile("coast.mbtiles", 1, 0, 0),
            Some((b"JPEGBYTES".to_vec(), "image/jpeg"))
        );
        assert_eq!(im.tile("coast.mbtiles", 1, 0, 1), None);
        assert_eq!(im.tile("other.mbtiles", 1, 0, 0), None);
        drop(im);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A minimal PMTiles v3 file: one PNG tile at z1 x1 y0, no compression.
    fn tiny_pmtiles() -> Vec<u8> {
        fn var(out: &mut Vec<u8>, mut v: u64) {
            loop {
                let b = (v & 0x7f) as u8;
                v >>= 7;
                if v == 0 {
                    out.push(b);
                    return;
                }
                out.push(b | 0x80);
            }
        }
        let tile = b"PNGBYTES".to_vec();
        let mut dir = Vec::new();
        var(&mut dir, 1); // entries
        var(&mut dir, tile_id(1, 1, 0)); // id delta
        var(&mut dir, 1); // run length
        var(&mut dir, tile.len() as u64); // length
        var(&mut dir, 1); // offset + 1
        let meta = br#"{"name":"Strait mosaic","attribution":"Imagery (c) Example Sat"}"#.to_vec();
        let root_offset = 127u64;
        let meta_offset = root_offset + dir.len() as u64;
        let data_offset = meta_offset + meta.len() as u64;
        let mut h = vec![0u8; 127];
        h[..7].copy_from_slice(b"PMTiles");
        h[7] = 3;
        let put =
            |h: &mut Vec<u8>, at: usize, v: u64| h[at..at + 8].copy_from_slice(&v.to_le_bytes());
        put(&mut h, 8, root_offset);
        put(&mut h, 16, dir.len() as u64);
        put(&mut h, 24, meta_offset);
        put(&mut h, 32, meta.len() as u64);
        put(&mut h, 40, data_offset);
        put(&mut h, 48, 0);
        put(&mut h, 56, data_offset);
        put(&mut h, 64, tile.len() as u64);
        h[97] = 1; // directories not compressed
        h[98] = 1; // tiles not compressed
        h[99] = 2; // PNG
        h[100] = 0;
        h[101] = 1;
        let e7 = |h: &mut Vec<u8>, at: usize, v: f64| {
            h[at..at + 4].copy_from_slice(&((v * 1e7) as i32).to_le_bytes());
        };
        e7(&mut h, 102, 119.0);
        e7(&mut h, 106, 11.0);
        e7(&mut h, 110, 122.0);
        e7(&mut h, 114, 14.0);
        let mut out = h;
        out.extend(dir);
        out.extend(meta);
        out.extend(tile);
        out
    }

    #[test]
    fn serves_a_pmtiles_pack_and_installs_only_tile_packs() {
        let dir = tmp("pm");
        let name = install(&dir, "Strait Mosaic.pmtiles", &tiny_pmtiles()).unwrap();
        assert_eq!(name, "Strait Mosaic.pmtiles");
        assert!(install(&dir, "notes.txt", b"hello").is_err());
        assert!(install(&dir, "bad.pmtiles", b"not a pack").is_err());
        assert!(!dir.join("bad.pmtiles").exists());
        let mut im = Imagery {
            folder: Some(dir.clone()),
            ..Imagery::default()
        };
        im.rescan();
        let p = im.status().packs[0].clone();
        assert_eq!(
            (p.name.as_str(), p.format.as_str()),
            ("Strait mosaic", "png")
        );
        assert_eq!(p.attribution.as_deref(), Some("Imagery (c) Example Sat"));
        assert_eq!(p.bounds, [119.0, 11.0, 122.0, 14.0]);
        assert_eq!(
            im.tile(&name, 1, 1, 0),
            Some((b"PNGBYTES".to_vec(), "image/png"))
        );
        assert_eq!(im.tile(&name, 1, 0, 0), None);
        drop(im);
        std::fs::remove_dir_all(&dir).ok();
    }
}
