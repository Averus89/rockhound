//! Signature table: which in-game objects produce which signature values.
//!
//! The table is plain JSON so it can be synced from the repository when a new
//! Star Citizen patch changes values, without shipping a new app build.

use serde::{Deserialize, Serialize};

/// Schema version this build understands. Bump on breaking format changes so
/// older apps ignore synced files they can't read.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum SignatureDbError {
    #[error("invalid signature table JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported signature table schema {found} (expected {SCHEMA_VERSION})")]
    Schema { found: u32 },
    #[error("signature source {0:?} has a zero base or max_count")]
    InvalidSource(String),
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SignatureMatch {
    pub name: String,
    pub category: Category,
    pub tier: Option<String>,
    pub base: u32,
    pub count: u32,
}

impl SignatureDb {
    /// Parse and validate a table.
    pub fn from_json(json: &str) -> Result<Self, SignatureDbError> {
        let db: Self = serde_json::from_str(json)?;
        if db.schema != SCHEMA_VERSION {
            return Err(SignatureDbError::Schema { found: db.schema });
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
                (1..=s.max_count).contains(&count).then(|| SignatureMatch {
                    name: s.name.clone(),
                    category: s.category,
                    tier: s.tier.clone(),
                    base: s.base,
                    count,
                })
            })
            .collect();
        out.sort_by_key(|m| m.count);
        out
    }

    pub fn is_valid(&self, value: u32) -> bool {
        self.sources
            .iter()
            .any(|s| value.is_multiple_of(s.base) && (1..=s.max_count).contains(&(value / s.base)))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn sample_db() -> SignatureDb {
        SignatureDb::from_json(
            r#"{
                "schema": 1, "data_version": "test", "sc_version": "4.7",
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
    fn rejects_other_schema() {
        let json = r#"{"schema": 99, "data_version": "x", "sc_version": "x", "sources": []}"#;
        assert!(matches!(
            SignatureDb::from_json(json),
            Err(SignatureDbError::Schema { found: 99 })
        ));
    }

    #[test]
    fn bundled_table_is_valid() {
        let json = include_str!("../../../data/signatures.json");
        let db = SignatureDb::from_json(json).unwrap();
        assert!(db.is_valid(3170));
    }
}
