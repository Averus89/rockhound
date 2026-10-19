//! Signature table: which in-game objects produce which signature values.
//!
//! The table is plain JSON so it can be synced from the repository when a new
//! Star Citizen patch changes values, without shipping a new app build.

use serde::{Deserialize, Serialize};

/// Schema version this build understands. Bump on breaking format changes so
/// older apps ignore synced files they can't read.
pub const SCHEMA_VERSION: u32 = 1;
/// Largest mixed cluster [`SignatureDb::mixes`] considers. Bigger clusters
/// have so many explanations that a list of them says nothing.
pub const MAX_MIX_ROCKS: u32 = 12;

#[derive(Debug, thiserror::Error)]
pub enum SignatureDbError {
    #[error("invalid signature table JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported signature table schema {found} (expected {SCHEMA_VERSION})")]
    Schema { found: u32 },
    #[error("signature source {0:?} has a zero base or max_count")]
    InvalidSource(String),
    #[error("data_version {0:?} is not a YYYY.MM.DD date")]
    DataVersion(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureDb {
    pub schema: u32,
    /// Version of the data itself, compared when syncing.
    pub data_version: String,
    pub sc_version: String,
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    pub category: Category,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// Signature of a single object; clusters read as `base * count`.
    pub base: u32,
    /// Largest plausible cluster, so huge values don't match every base.
    pub max_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    ShipMining,
    GroundDeposit,
    Salvage,
}

impl Category {
    /// Where this kind of object can be found, or `None` for both.
    /// Ground deposits only exist on planet and moon surfaces; salvage is
    /// wreckage in space. Ship-mining rocks are both asteroids and surface
    /// rocks, so they never move when the player's location changes.
    pub fn found_in(self) -> Option<Location> {
        match self {
            Self::ShipMining => None,
            Self::GroundDeposit => Some(Location::Planet),
            Self::Salvage => Some(Location::Space),
        }
    }
}

/// Where the player is scanning. Values like 3000 are both a ground deposit
/// and a capital wreck, so knowing this picks the likely one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Location {
    #[default]
    Unknown,
    Space,
    Planet,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SignatureMatch {
    pub name: String,
    pub category: Category,
    pub tier: Option<String>,
    pub base: u32,
    pub count: u32,
}

impl Source {
    fn matched(&self, count: u32) -> SignatureMatch {
        SignatureMatch {
            name: self.name.clone(),
            category: self.category,
            tier: self.tier.clone(),
            base: self.base,
            count,
        }
    }
}

impl SignatureDb {
    /// Parse and validate a table.
    pub fn from_json(json: &str) -> Result<Self, SignatureDbError> {
        let db: Self = serde_json::from_str(json)?;
        if db.schema != SCHEMA_VERSION {
            return Err(SignatureDbError::Schema { found: db.schema });
        }
        // Sync compares versions as strings, which is only time order for
        // this exact format; a typo like "9999.x" would block later updates.
        if !is_date_version(&db.data_version) {
            return Err(SignatureDbError::DataVersion(db.data_version));
        }
        if let Some(bad) = db.sources.iter().find(|s| s.base == 0 || s.max_count == 0) {
            return Err(SignatureDbError::InvalidSource(bad.name.clone()));
        }
        Ok(db)
    }

    /// Everything that could produce `value`, single objects first.
    pub fn matches(&self, value: u32) -> Vec<SignatureMatch> {
        let mut out: Vec<SignatureMatch> = self
            .sources
            .iter()
            .filter(|s| value.is_multiple_of(s.base))
            .filter_map(|s| {
                let count = value / s.base;
                (1..=s.max_count).contains(&count).then(|| s.matched(count))
            })
            .collect();
        out.sort_by_key(|m| m.count);
        out
    }

    /// [`Self::matches`], with sources that don't belong at `location`
    /// moved to the end. They stay listed, since the location may be wrong.
    pub fn matches_at(&self, value: u32, location: Location) -> Vec<SignatureMatch> {
        let mut out = self.matches(value);
        rank_for(&mut out, location);
        out
    }

    /// Ways rocks of two different minerals could add up to `value`, fewest
    /// rocks first, then same-tier pairs (they tend to share a field).
    /// Only ship-mining sources mix; each pair lists its smaller base first.
    pub fn mixes(&self, value: u32) -> Vec<[SignatureMatch; 2]> {
        let mining: Vec<&Source> = self
            .sources
            .iter()
            .filter(|s| s.category == Category::ShipMining)
            .collect();
        let mut out = Vec::new();
        for (i, a) in mining.iter().enumerate() {
            for b in &mining[i + 1..] {
                for count_a in 1..MAX_MIX_ROCKS {
                    let Some(rest) = value.checked_sub(a.base * count_a).filter(|r| *r > 0) else {
                        break;
                    };
                    let count_b = rest / b.base;
                    if rest.is_multiple_of(b.base) && count_a + count_b <= MAX_MIX_ROCKS {
                        let mut pair = [a.matched(count_a), b.matched(count_b)];
                        pair.sort_by_key(|m| m.base);
                        out.push(pair);
                    }
                }
            }
        }
        out.sort_by(|x, y| {
            let rocks = |p: &[SignatureMatch; 2]| p[0].count + p[1].count;
            let mixed_tier = |p: &[SignatureMatch; 2]| p[0].tier != p[1].tier;
            (rocks(x), mixed_tier(x), &x[0].name, &x[1].name).cmp(&(
                rocks(y),
                mixed_tier(y),
                &y[0].name,
                &y[1].name,
            ))
        });
        out
    }

    pub fn is_valid(&self, value: u32) -> bool {
        self.sources
            .iter()
            .any(|s| value.is_multiple_of(s.base) && (1..=s.max_count).contains(&(value / s.base)))
    }
}

/// Move matches that don't belong at `location` to the end, keeping the
/// order within each group.
fn rank_for(matches: &mut [SignatureMatch], location: Location) {
    if location != Location::Unknown {
        matches.sort_by_key(|m| m.category.found_in().is_some_and(|l| l != location));
    }
}

fn is_date_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    let well_formed = matches!(parts.as_slice(), [y, m, d] if y.len() == 4 && m.len() == 2 && d.len() == 2)
        && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()));
    well_formed
        && (1..=12).contains(&parts[1].parse::<u32>().unwrap_or(0))
        && (1..=31).contains(&parts[2].parse::<u32>().unwrap_or(0))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn sample_db() -> SignatureDb {
        SignatureDb::from_json(
            r#"{
                "schema": 1, "data_version": "2026.01.01", "sc_version": "4.7",
                "sources": [
                    {"name": "Laranite", "category": "ship_mining", "tier": "uncommon", "base": 3825, "max_count": 100},
                    {"name": "Riccite", "category": "ship_mining", "tier": "epic", "base": 3385, "max_count": 100},
                    {"name": "Salvage Panels", "category": "salvage", "base": 2000, "max_count": 100},
                    {"name": "Small Ground Deposit", "category": "ground_deposit", "base": 3000, "max_count": 50},
                    {"name": "Copper", "category": "ship_mining", "tier": "common", "base": 4240, "max_count": 100}
                ]
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn matches_cluster_counts() {
        let db = sample_db();
        let m = db.matches(16960);
        assert_eq!(m.len(), 1);
        assert_eq!((m[0].name.as_str(), m[0].count), ("Copper", 4));
        assert!(db.matches(16980).is_empty());
    }

    #[test]
    fn shared_values_list_all_sources_smallest_count_first() {
        let m = sample_db().matches(6000);
        let names: Vec<&str> = m.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["Small Ground Deposit", "Salvage Panels"]);
    }

    #[test]
    fn location_ranks_colliding_sources_without_dropping_any() {
        let db = SignatureDb::from_json(include_str!("../../../data/signatures.json")).unwrap();
        let names = |value, location| -> Vec<String> {
            db.matches_at(value, location)
                .into_iter()
                .map(|m| m.name)
                .collect()
        };
        // Single objects first when the location is unknown, as before.
        assert_eq!(
            names(3000, Location::Unknown),
            ["Small Ground Deposit", "Capital Wreck Debris"]
        );
        assert_eq!(
            names(3000, Location::Space),
            ["Capital Wreck Debris", "Small Ground Deposit"]
        );
        assert_eq!(
            names(4000, Location::Space),
            ["Salvage Panels", "Large Ground Deposit"]
        );
        assert_eq!(
            names(4000, Location::Planet),
            ["Large Ground Deposit", "Salvage Panels"]
        );
        // Ship rocks are found everywhere, so they keep their place.
        assert_eq!(
            names(12720, Location::Space),
            names(12720, Location::Planet)
        );
    }

    #[test]
    fn mixes_two_minerals_fewest_rocks_first() {
        // 2 Laranite + 3 Copper = 7650 + 12720.
        let m = sample_db().mixes(20370);
        let first: Vec<(&str, u32)> = m[0].iter().map(|p| (p.name.as_str(), p.count)).collect();
        assert_eq!(first, vec![("Laranite", 2), ("Copper", 3)]);
        assert!(m
            .iter()
            .all(|p| p.iter().all(|x| x.category == Category::ShipMining)));
        assert!(sample_db().mixes(20371).is_empty());
        assert!(sample_db().mixes(4240).is_empty());
    }

    #[test]
    fn rejects_other_schema() {
        let json = r#"{"schema": 99, "data_version": "x", "sc_version": "x", "sources": []}"#;
        assert!(matches!(
            SignatureDb::from_json(json),
            Err(SignatureDbError::Schema { found: 99 })
        ));
    }

    #[test]
    fn rejects_malformed_data_version() {
        for bad in ["9999.bad", "2026.13.01", "2026.3.18", "2026.03.18.1"] {
            let json = format!(
                r#"{{"schema": 1, "data_version": "{bad}", "sc_version": "x", "sources": []}}"#
            );
            assert!(
                matches!(
                    SignatureDb::from_json(&json),
                    Err(SignatureDbError::DataVersion(_))
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn bundled_table_is_valid() {
        let json = include_str!("../../../data/signatures.json");
        let db = SignatureDb::from_json(json).unwrap();
        assert!(db.is_valid(3170));
    }
}
