//! `sim-db`: build, validate, migrate and inspect the platform database.
//!
//! ```text
//! cargo run -p sim-db -- validate                 # build data/db/open in memory and validate it
//! cargo run -p sim-db -- build --out open.sqlite  # build + validate, then write the file
//! cargo run -p sim-db -- list                     # platforms by category
//! cargo run -p sim-db -- migrate --db file.sqlite
//! ```

#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use rusqlite::Connection;
use sim_db::validate::{counts, validate};
use sim_db::{migrate, source, Policy, Severity};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "sim-db",
    version,
    about = "Naval wargame platform database tool"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Validate the database. By default, builds the open source tree in memory first.
    Validate {
        /// JSON source directory (default: the repository's data/db/open).
        #[arg(long, conflicts_with = "db")]
        source: Option<PathBuf>,
        /// Validate an existing SQLite file instead of building from source.
        #[arg(long)]
        db: Option<PathBuf>,
        /// Treat warnings as errors.
        #[arg(long)]
        strict: bool,
    },
    /// Build a SQLite database from source. Refuses to write it if validation reports errors.
    Build {
        #[arg(long)]
        source: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
    /// Apply pending migrations to an existing database file.
    Migrate {
        #[arg(long)]
        db: PathBuf,
    },
    /// List platforms by category.
    List {
        #[arg(long)]
        source: Option<PathBuf>,
    },
}

fn default_source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/db/open")
}

fn build_in_memory(dir: &Path) -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    let stats = source::build(&mut conn, dir)
        .with_context(|| format!("building database from {}", dir.display()))?;
    eprintln!(
        "built {} platforms, {} sensors, {} weapons from {} files in {}",
        stats.platforms,
        stats.sensors,
        stats.weapons,
        stats.files,
        dir.display()
    );
    Ok(conn)
}

/// Prints the issues and returns the number of errors (warnings count when `strict`).
fn report(conn: &Connection, strict: bool) -> Result<usize> {
    let issues = validate(conn, Policy::Open)?;
    for i in &issues {
        println!("{i}");
    }
    let c = counts(&issues);
    let errors = c.get(&Severity::Error).copied().unwrap_or(0);
    let warnings = c.get(&Severity::Warning).copied().unwrap_or(0);
    println!("validation: {errors} error(s), {warnings} warning(s)");
    Ok(if strict { errors + warnings } else { errors })
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Cmd::Validate { source, db, strict } => {
            let conn = match db {
                Some(path) => Connection::open(&path)
                    .with_context(|| format!("opening {}", path.display()))?,
                None => build_in_memory(&source.unwrap_or_else(default_source))?,
            };
            if report(&conn, strict)? > 0 {
                bail!("database validation failed");
            }
        }
        Cmd::Build { source, out } => {
            let dir = source.unwrap_or_else(default_source);
            let conn = build_in_memory(&dir)?;
            if report(&conn, false)? > 0 {
                bail!("not writing {}: validation failed", out.display());
            }
            if out.exists() {
                std::fs::remove_file(&out)
                    .with_context(|| format!("replacing {}", out.display()))?;
            }
            conn.execute("VACUUM INTO ?1", [out.to_string_lossy()])
                .with_context(|| format!("writing {}", out.display()))?;
            println!("wrote {}", out.display());
        }
        Cmd::Migrate { db } => {
            let mut conn =
                Connection::open(&db).with_context(|| format!("opening {}", db.display()))?;
            let before = migrate::schema_version(&conn)?;
            let after = migrate::migrate(&mut conn)?;
            println!("{}: schema version {before} -> {after}", db.display());
        }
        Cmd::List { source } => {
            let conn = build_in_memory(&source.unwrap_or_else(default_source))?;
            let mut stmt = conn.prepare(
                "SELECT category, id, name, max_speed_mps FROM platforms ORDER BY category, id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, f64>(3)?,
                ))
            })?;
            for row in rows {
                let (cat, id, name, v) = row?;
                println!(
                    "{cat:<18} {id:<16} {name:<46} {:>6.1} kn",
                    v * 3600.0 / 1852.0
                );
            }
        }
    }
    Ok(())
}
