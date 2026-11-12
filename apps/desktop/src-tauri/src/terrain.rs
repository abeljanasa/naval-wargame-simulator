//! Terrain and bathymetry packs on this machine (Phase 4 slice 2,
//! `docs/models/terrain-and-bathymetry.md`; user manual §2, *Terrain packs*).
//!
//! Packs are looked for in two folders: the one bundled with the app (the coarse global layer)
//! and the user's installed-packs folder (regional packs). A pack with a `.sha256` file next to
//! it must match it, or it is not used. Installing a pack from the app checks it against the
//! SHA-256 the user supplies (from the `.sha256` file shipped with it) before copying it in.

use serde::Serialize;
use sim_geo::terrain::{sha256_hex, Terrain, TerrainPack, PACK_EXTENSION};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Largest pack the app installs, bytes.
pub const MAX_PACK_BYTES: usize = 1 << 30;

/// One pack found on this machine.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct TerrainPackInfo {
    pub id: String,
    pub name: String,
    pub sha256: String,
    pub source: String,
    pub credit: String,
    pub cell_arcsec: f64,
    /// South, west, north, east, degrees.
    pub bounds_deg: [f64; 4],
    /// Shipped with the app, rather than installed by the user.
    pub bundled: bool,
}

/// The terrain packs on this machine and the ones the current exercise uses.
#[derive(Debug, Clone, Default, Serialize, specta::Type)]
pub struct TerrainStatus {
    /// Still reading the packs (at start-up).
    pub loading: bool,
    pub packs: Vec<TerrainPackInfo>,
    /// The terrain id the current exercise runs with (`None`: none).
    pub in_use: Option<String>,
    /// Packs that were found but not used, and why.
    pub problems: Vec<String>,
    /// Where installed packs are kept.
    pub folder: String,
}

/// Where packs are kept.
#[derive(Debug, Clone)]
pub struct TerrainDirs {
    pub bundled: Option<PathBuf>,
    pub installed: PathBuf,
}

/// What was found in the folders.
#[derive(Debug, Default)]
pub struct Loaded {
    pub packs: Vec<(Arc<TerrainPack>, bool)>,
    pub problems: Vec<String>,
}

impl Loaded {
    #[must_use]
    pub fn terrain(&self) -> Option<Arc<Terrain>> {
        Terrain::new(self.packs.iter().map(|(p, _)| p.clone()).collect()).map(Arc::new)
    }

    #[must_use]
    pub fn infos(&self) -> Vec<TerrainPackInfo> {
        let mut v: Vec<TerrainPackInfo> = self
            .packs
            .iter()
            .map(|(p, bundled)| {
                let h = p.header();
                TerrainPackInfo {
                    id: p.id().to_owned(),
                    name: h.name.clone(),
                    sha256: p.sha256().to_owned(),
                    source: h.source.clone(),
                    credit: h.credit.clone(),
                    cell_arcsec: h.cell_arcsec(),
                    bounds_deg: h.bounds_deg(),
                    bundled: *bundled,
                }
            })
            .collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    }
}

/// Reads the SHA-256 from a `sha256sum`-style line (`<hex>  <file>`).
fn sidecar_sha(text: &str) -> Option<String> {
    let hex = text.split_whitespace().next()?.to_ascii_lowercase();
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hex)
}

fn load_one(path: &Path) -> Result<TerrainPack, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let sidecar = path.with_file_name(format!(
        "{}.sha256",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    if let Ok(text) = std::fs::read_to_string(&sidecar) {
        let expected = sidecar_sha(&text).ok_or("its .sha256 file is not a SHA-256 line")?;
        let actual = sha256_hex(&bytes);
        if actual != expected {
            return Err(format!(
                "SHA-256 is {actual}, but its .sha256 file says {expected} (damaged or altered)"
            ));
        }
    }
    TerrainPack::decode(&bytes).map_err(|e| e.to_string())
}

/// Reads every pack in the folders. Problems are reported, not fatal.
#[must_use]
pub fn load(dirs: &TerrainDirs) -> Loaded {
    let mut out = Loaded::default();
    let folders = dirs
        .bundled
        .iter()
        .map(|d| (d.as_path(), true))
        .chain(std::iter::once((dirs.installed.as_path(), false)));
    for (dir, bundled) in folders {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == PACK_EXTENSION))
            .collect();
        files.sort();
        for f in files {
            match load_one(&f) {
                Ok(p) => {
                    if !out.packs.iter().any(|(q, _)| q.id() == p.id()) {
                        out.packs.push((Arc::new(p), bundled));
                    }
                }
                Err(e) => out.problems.push(format!(
                    "{}: {e}",
                    f.file_name().unwrap_or_default().to_string_lossy()
                )),
            }
        }
    }
    out
}

/// Checks a pack against the SHA-256 the user gave and copies it into the installed folder,
/// as `<pack name>.nwsterrain` with its `.sha256` next to it. Returns the pack's id.
pub fn install(dirs: &TerrainDirs, bytes: &[u8], expected_sha: &str) -> Result<String, String> {
    if bytes.len() > MAX_PACK_BYTES {
        return Err(format!(
            "the pack is {} bytes; the limit is {MAX_PACK_BYTES}",
            bytes.len()
        ));
    }
    let expected = sidecar_sha(expected_sha)
        .ok_or("choose the pack's .sha256 file together with the pack, to check it")?;
    let actual = sha256_hex(bytes);
    if actual != expected {
        return Err(format!(
            "the pack's SHA-256 is {actual}, but its .sha256 file says {expected}: \
             the copy is damaged or altered, so it was not installed"
        ));
    }
    let pack = TerrainPack::decode(bytes).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dirs.installed)
        .map_err(|e| format!("cannot create {}: {e}", dirs.installed.display()))?;
    let file = format!("{}.{PACK_EXTENSION}", pack.header().name);
    let path = dirs.installed.join(&file);
    // The pack first, then its .sha256: a pack without one still loads; never the reverse.
    let tmp = dirs.installed.join(format!("{file}.part"));
    std::fs::write(&tmp, bytes).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    std::fs::write(
        dirs.installed.join(format!("{file}.sha256")),
        format!("{actual}  {file}\n"),
    )
    .map_err(|e| format!("cannot write the .sha256 file: {e}"))?;
    Ok(pack.id().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_geo::terrain::{encode, PackHeader};

    fn pack_bytes(name: &str, h: i16) -> Vec<u8> {
        encode(
            &PackHeader {
                name: name.into(),
                source: "test".into(),
                credit: "test".into(),
                lat0: 0,
                lon0: 0,
                cell: 36_000,
                rows: 2,
                cols: 2,
                min_m: h,
                max_m: h,
            },
            &[h; 4],
        )
        .unwrap()
    }

    #[test]
    fn installs_only_a_pack_that_matches_its_sha256() {
        let root = std::env::temp_dir().join(format!("nws-terrain-dirs-{}", std::process::id()));
        let dirs = TerrainDirs {
            bundled: None,
            installed: root.join("installed"),
        };
        let bytes = pack_bytes("shelf", -40);
        let sha = sha256_hex(&bytes);
        assert!(install(&dirs, &bytes, "").is_err(), "no SHA-256 given");
        let wrong = sha256_hex(b"something else");
        assert!(install(&dirs, &bytes, &wrong)
            .unwrap_err()
            .contains("damaged"));
        let id = install(&dirs, &bytes, &format!("{sha}  shelf.nwsterrain\n")).unwrap();
        let loaded = load(&dirs);
        assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
        assert_eq!(loaded.infos()[0].id, id);
        assert_eq!(loaded.terrain().unwrap().id(), id);

        // A pack altered after installation is refused at the next start, and reported.
        std::fs::write(
            dirs.installed.join("shelf.nwsterrain"),
            pack_bytes("shelf", -41),
        )
        .unwrap();
        let loaded = load(&dirs);
        assert!(loaded.packs.is_empty());
        assert!(loaded.problems[0].contains("damaged or altered"));
        std::fs::remove_dir_all(&root).ok();
    }
}
