//! User settings, stored as JSON in the app config folder.

use std::path::Path;

use rockhound_core::Location;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Readout box in game-window pixels: x, y, width, height.
    pub region: Option<[u32; 4]>,
    /// Time between reads. 250 ms keeps the UI snappy at negligible CPU now
    /// that a read costs a few milliseconds.
    pub interval_ms: u64,
    /// Show the click-through overlay over the game.
    pub overlay: bool,
    /// Save what the scanner sees, for bug reports.
    pub debug: bool,
    /// Keep read crops as glyph samples the player can send in.
    pub glyph_samples: bool,
    /// Where the player is, so colliding signatures rank the likely one first.
    pub location: Location,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            region: None,
            interval_ms: 250,
            overlay: false,
            debug: false,
            glyph_samples: false,
            location: Location::Unknown,
        }
    }
}

impl Settings {
    /// Load settings, falling back to defaults when the file is missing or
    /// unreadable so a bad file never blocks startup.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| format!("cannot save settings: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_bad_file_fallback() {
        let dir = std::env::temp_dir().join(format!("rockhound-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let s = Settings {
            region: Some([1, 2, 3, 4]),
            interval_ms: 100,
            overlay: true,
            debug: true,
            glyph_samples: true,
            location: Location::Planet,
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::remove_dir_all(dir).ok();
    }
}
