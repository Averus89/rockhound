//! Signature table sync.
//!
//! The app ships a bundled table, keeps the last downloaded table in its data
//! folder, and checks the repository for a newer one at startup. A new SC
//! patch then only needs a commit to `data/signatures.json`.

use std::path::Path;
use std::time::Duration;

use rockhound_core::SignatureDb;

pub const BUNDLED: &str = include_str!("../../../data/signatures.json");
pub const REMOTE_URL: &str =
    "https://raw.githubusercontent.com/Averus89/rockhound/main/data/signatures.json";
const TIMEOUT: Duration = Duration::from_secs(10);

/// `data_version` is a date (`YYYY.MM.DD`), so string order is time order.
fn is_newer(candidate: &SignatureDb, current: &SignatureDb) -> bool {
    candidate.data_version > current.data_version
}

/// The newest valid table among the bundled copy and the cache.
pub fn load_local(cache: &Path) -> SignatureDb {
    let bundled = SignatureDb::from_json(BUNDLED).expect("bundled signature table is valid");
    let cached = std::fs::read_to_string(cache)
        .ok()
        .and_then(|text| SignatureDb::from_json(&text).ok());
    match cached {
        Some(c) if is_newer(&c, &bundled) => c,
        _ => bundled,
    }
}

/// Download the remote table. Returns it when it is valid and newer than
/// `current`, after writing it to the cache.
pub fn update(cache: &Path, current: &SignatureDb) -> Result<Option<SignatureDb>, String> {
    let text = fetch(REMOTE_URL)?;
    apply(&text, cache, current)
}

fn fetch(url: &str) -> Result<String, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .build()
        .new_agent();
    agent
        .get(url)
        .call()
        .map_err(|e| format!("signature sync failed: {e}"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("signature sync failed: {e}"))
}

fn apply(text: &str, cache: &Path, current: &SignatureDb) -> Result<Option<SignatureDb>, String> {
    let remote = SignatureDb::from_json(text).map_err(|e| e.to_string())?;
    if !is_newer(&remote, current) {
        return Ok(None);
    }
    if let Some(dir) = cache.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(cache, text).map_err(|e| format!("cannot cache signature table: {e}"))?;
    Ok(Some(remote))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_version(version: &str) -> String {
        BUNDLED.replacen(
            &format!("\"data_version\": \"{}\"", bundled().data_version),
            &format!("\"data_version\": \"{version}\""),
            1,
        )
    }

    fn bundled() -> SignatureDb {
        SignatureDb::from_json(BUNDLED).unwrap()
    }

    fn temp_cache(name: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("rockhound-sync-{}-{name}", std::process::id()))
            .join("signatures.json")
    }

    #[test]
    fn newer_remote_is_cached_and_preferred() {
        let cache = temp_cache("newer");
        let updated = apply(&with_version("2099.01.01"), &cache, &bundled()).unwrap();
        assert_eq!(updated.unwrap().data_version, "2099.01.01");
        assert_eq!(load_local(&cache).data_version, "2099.01.01");
        std::fs::remove_dir_all(cache.parent().unwrap()).ok();
    }

    #[test]
    fn older_or_invalid_remote_is_ignored() {
        let cache = temp_cache("older");
        assert!(apply(&with_version("2000.01.01"), &cache, &bundled())
            .unwrap()
            .is_none());
        assert!(apply("{}", &cache, &bundled()).is_err());
        assert!(!cache.exists());
        assert_eq!(load_local(&cache).data_version, bundled().data_version);
    }
}
