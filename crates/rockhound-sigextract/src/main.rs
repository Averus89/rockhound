//! `sigextract`: refresh `data/signatures.json` from the installed game.
//!
//! 1. `update`  merge the game's values into the table (the usual command).
//! 2. `list`    print every entity's signature, to see what the game has.
//! 3. `dump`    print one record as JSON, to find a moved field after a patch.
//! 4. `export`  save the decompressed Game2.dcb, to debug the parser offline.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use rockhound_core::SignatureDb;
use rockhound_sigextract::{entities, game_version, load, read_dcb, report, resolve_input, table};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Data.p4k, a loose Game2.dcb, or the game folder (LIVE, PTU, ...).
    #[arg(long, global = true, default_value = rockhound_sigextract::DEFAULT_GAME_DIR)]
    game: PathBuf,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Update the signature table with values from the game.
    Update {
        #[arg(long, default_value = "data/signatures.json")]
        table: PathBuf,
        /// Game version to record; read from build_manifest.id when omitted.
        #[arg(long)]
        sc_version: Option<String>,
        /// Show what would change without writing the table.
        #[arg(long)]
        dry_run: bool,
    },
    /// Print every entity's signature as tab-separated values.
    List {
        /// Only entities whose name or file contains this text.
        #[arg(long)]
        filter: Option<String>,
    },
    /// Print records whose name contains NAME as JSON.
    Dump {
        name: String,
        /// How many pointers deep to follow.
        #[arg(long, default_value_t = 4)]
        depth: u32,
    },
    /// Save the decompressed Game2.dcb without parsing it.
    Export {
        #[arg(long, default_value = "Game2.dcb")]
        out: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let input = resolve_input(&cli.game);
    eprintln!("Reading {} ...", input.display());
    if let Cmd::Export { out } = &cli.command {
        let bytes = read_dcb(&input)?;
        std::fs::write(out, &bytes).with_context(|| format!("cannot write {}", out.display()))?;
        println!("Wrote {} ({} bytes).", out.display(), bytes.len());
        return Ok(());
    }
    let db = load(&input)?;
    match cli.command {
        Cmd::Update {
            table,
            sc_version,
            dry_run,
        } => update(&db, &input, &table, sc_version, dry_run),
        Cmd::List { filter } => {
            let (found, _) = entities::entity_signatures(&db)?;
            print!("{}", report::entity_tsv(&found, filter.as_deref()));
            Ok(())
        }
        Cmd::Export { .. } => unreachable!("handled before parsing"),
        Cmd::Dump { name, depth } => {
            let mut shown = 0;
            for rec in db.records().filter(|r| r.name.contains(&name)) {
                println!("// {} ({})", rec.name, rec.file_name);
                println!("{:#}", db.to_json(rec.ptr, depth)?);
                shown += 1;
            }
            if shown == 0 {
                bail!("no record name contains {name:?}");
            }
            Ok(())
        }
    }
}

fn update(
    db: &rockhound_sigextract::DataCore,
    input: &Path,
    table_path: &Path,
    sc_version: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let text = std::fs::read_to_string(table_path)
        .with_context(|| format!("cannot read {}", table_path.display()))?;
    let current = SignatureDb::from_json(&text)?;
    let (found, scan) = entities::entity_signatures(db)?;
    eprintln!(
        "{} entities, {} with signatures",
        scan.entities, scan.with_signature
    );
    if found.is_empty() {
        bail!("no entity signatures found; the game data layout may have changed (try `dump`)");
    }
    let sc_version = sc_version.or_else(|| game_version(input));
    let today = chrono::Local::now().format("%Y.%m.%d").to_string();
    let merge = table::merge(&current, &found, sc_version.as_deref(), &today);
    print!("{}", report::merge_report(&merge));

    if !merge.changed {
        println!("{} is up to date.", table_path.display());
        return Ok(());
    }
    if merge.table.data_version <= current.data_version {
        eprintln!(
            "warning: data_version {} is not newer than {}; installed apps will not pick this up",
            merge.table.data_version, current.data_version
        );
    }
    if dry_run {
        println!("Dry run: {} not written.", table_path.display());
        return Ok(());
    }
    std::fs::write(table_path, table::to_json(&merge.table))
        .with_context(|| format!("cannot write {}", table_path.display()))?;
    println!(
        "Wrote {} (data_version {}, sc_version {}).",
        table_path.display(),
        merge.table.data_version,
        merge.table.sc_version
    );
    Ok(())
}
