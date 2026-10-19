//! Extract signature values from Star Citizen's game files.
//!
//! Reads `Data/Game2.dcb` out of `Data.p4k`, finds every entity's radar
//! signature and merges the values into Rockhound's `data/signatures.json`.
//! Based on the method Mallachi documented for SC Signature Scanner.

pub mod datacore;
pub mod entities;
pub mod p4k;
pub mod report;
pub mod table;

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub use datacore::DataCore;

/// Where the RSI launcher installs the live game by default.
pub const DEFAULT_GAME_DIR: &str = r"C:\Program Files\Roberts Space Industries\StarCitizen\LIVE";
const DCB_NAME: &str = "data/game2.dcb";

/// The archive or database `path` points at: a `Data.p4k`, a loose
/// `Game2.dcb`, or the game folder holding `Data.p4k`.
pub fn resolve_input(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join("Data.p4k")
    } else {
        path.to_owned()
    }
}

/// Load DataCore from a `Data.p4k` or a loose `.dcb` file.
pub fn load(path: &Path) -> Result<DataCore> {
    let is_dcb = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("dcb"));
    let bytes = if is_dcb {
        std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?
    } else {
        let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
        let (_, bytes) = p4k::read_entry(BufReader::new(file), is_dcb_name, "Data/Game2.dcb")
            .with_context(|| format!("reading {}", path.display()))?;
        bytes
    };
    Ok(DataCore::parse(bytes)?)
}

fn is_dcb_name(name: &str) -> bool {
    name.replace('\\', "/").eq_ignore_ascii_case(DCB_NAME)
}

/// Game version from the launcher's `build_manifest.id` next to Data.p4k,
/// such as `4.7.1` for branch `sc-alpha-4.7.1`.
pub fn game_version(archive: &Path) -> Option<String> {
    let manifest = archive.parent()?.join("build_manifest.id");
    let text = std::fs::read_to_string(manifest).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let branch = json["Data"]["Branch"].as_str()?;
    Some(
        branch
            .strip_prefix("sc-alpha-")
            .unwrap_or(branch)
            .to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::tests::{channels, game};
    use crate::p4k::tests::{build_p4k, TestFile};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sigextract-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn loads_from_p4k_dcb_or_folder() {
        let dir = temp_dir("load");
        let dcb = game(&[("MineableRock_Gold", "gold.xml", channels(3585.0))]);
        std::fs::write(dir.join("Game2.dcb"), &dcb).unwrap();
        let archive = build_p4k(&[TestFile {
            name: "Data\\Game2.dcb",
            data: &dcb,
            method: 100,
        }]);
        std::fs::write(dir.join("Data.p4k"), archive).unwrap();

        for path in [dir.join("Game2.dcb"), resolve_input(&dir)] {
            let db = load(&path).unwrap();
            let (found, _) = entities::entity_signatures(&db).unwrap();
            assert_eq!(found[0].cross_section(), Some(3585));
        }
        let err = load(&dir.join("missing.p4k")).err().unwrap();
        assert!(err.to_string().contains("cannot open"));
        assert!(load(&dir.join("missing.dcb")).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_the_game_version_from_the_manifest() {
        let dir = temp_dir("manifest");
        let archive = dir.join("Data.p4k");
        assert_eq!(game_version(&archive), None);
        std::fs::write(
            dir.join("build_manifest.id"),
            r#"{"Data": {"Branch": "sc-alpha-4.7.1", "BuildId": "1"}}"#,
        )
        .unwrap();
        assert_eq!(game_version(&archive).as_deref(), Some("4.7.1"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn matches_dcb_names_with_either_separator() {
        assert!(is_dcb_name("Data\\Game2.dcb"));
        assert!(is_dcb_name("Data/Game2.dcb"));
        assert!(!is_dcb_name("Data/Other/Game2.dcb"));
    }
}
