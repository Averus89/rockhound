//! Merge extracted entity signatures into the app's signature table.
//!
//! The game has several entities per table row (asteroid and surface rocks,
//! size variants), so each row takes the most common value among the
//! entities that match it. Cluster limits are not in the game data and are
//! kept from the existing table.
//!
//! Entity names as of SC 4.7 (Game2.dcb version 8):
//! - `MineableRock_Asteroid<Tier>_<Mineral>[_...]` and
//!   `MineableRock_Surface<Tier>_<Mineral>`: ship mining, one value per
//!   mineral. A mineral missing from the table is added with its tier.
//! - `MineableRock_FPS_*` / `MineableRock_GroundVehicle_*`: small and large
//!   ground deposits.
//! - `SalvageScrap*`, `SalvageLock*`, `SalvageableRepairable_*`: panels.
//! - `SalvageableDebris_<Ship>`: wreck debris by hull size.
//!
//! The older `Asteroid<X>TypeMineableRock`, `<Rock>MineableRock_*`, test and
//! template entities are leftovers that never spawn and are ignored.

use std::collections::BTreeMap;

use rockhound_core::signatures::{Category, Source};
use rockhound_core::SignatureDb;

use crate::entities::EntitySignature;

/// Game spellings that differ from the table's names.
const ALIASES: [(&str, &str); 1] = [("aluminium", "Aluminum")];
const TIERS: [&str; 5] = ["common", "uncommon", "rare", "epic", "legendary"];
const SMALL_DEPOSIT: &str = "Small Ground Deposit";
const LARGE_DEPOSIT: &str = "Large Ground Deposit";
const SALVAGE_PANELS: &str = "Salvage Panels";
const PANEL_PREFIXES: [&str; 3] = ["salvagescrap", "salvagelock", "salvageablerepairable_"];
/// Wreck debris rows by the ship the debris comes from.
const WRECKS: [(&str, &str); 4] = [
    ("avengertitan", "Small Wreck Debris"),
    ("aresinferno", "Medium Wreck Debris"),
    ("c2", "Large Wreck Debris"),
    ("890", "Capital Wreck Debris"),
];
/// Families the rules above cover; members no rule claims are reported.
const FAMILIES: [&str; 3] = ["mineablerock_", "salvage", "salvageabledebris"];
/// `max_count` for minerals added from the game data, as for existing ones.
const NEW_MINERAL_MAX_COUNT: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub name: String,
    pub old: u32,
    pub new: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub value: u32,
    pub count: usize,
    pub examples: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Merge {
    pub table: SignatureDb,
    pub changes: Vec<Change>,
    /// Minerals found in the game but not in the table, now added.
    pub added: Vec<String>,
    /// Rows whose entities agree with the table.
    pub confirmed: Vec<(String, usize)>,
    /// Rows no entity matched, left as they were.
    pub unmatched_rows: Vec<String>,
    /// Rows whose entities disagree: every value seen, most common first.
    pub conflicts: Vec<(String, Vec<Group>)>,
    /// Mining or salvage entities no rule claimed, by value.
    pub unclaimed: Vec<Group>,
    /// Whether anything in the table changed (values, rows or game version).
    pub changed: bool,
}

/// What an entity is, by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A fixed table row.
    Row(&'static str),
    /// A ship-mineable mineral, with its name as the game spells it.
    Mineral { name: String, tier: &'static str },
    /// In a covered family, but no rule knows it.
    Unknown,
}

pub fn classify(entity_name: &str) -> Option<Kind> {
    let lower = entity_name.to_lowercase();
    let words: Vec<&str> = lower.split('_').collect();
    // `SalvageableDebris` on its own is the base the wreck entities inherit.
    if lower == "salvageabledebris" || words.iter().any(|w| *w == "template" || *w == "test") {
        return None;
    }
    if let ["mineablerock", family, mineral, ..] = words.as_slice() {
        if *family == "fps" {
            return Some(Kind::Row(SMALL_DEPOSIT));
        }
        if *family == "groundvehicle" {
            return Some(Kind::Row(LARGE_DEPOSIT));
        }
        let tier = ["asteroid", "surface"]
            .iter()
            .find_map(|p| family.strip_prefix(p))
            .and_then(|t| TIERS.iter().find(|x| **x == t));
        if let Some(tier) = tier {
            let original = entity_name.split('_').nth(2).unwrap_or(mineral);
            return Some(Kind::Mineral {
                name: original.to_owned(),
                tier,
            });
        }
    }
    if PANEL_PREFIXES.iter().any(|p| lower.starts_with(p)) {
        return Some(Kind::Row(SALVAGE_PANELS));
    }
    if let Some(ship) = lower.strip_prefix("salvageabledebris_") {
        if let Some((_, row)) = WRECKS.iter().find(|(s, _)| *s == ship) {
            return Some(Kind::Row(row));
        }
    }
    FAMILIES
        .iter()
        .any(|f| lower.starts_with(f))
        .then_some(Kind::Unknown)
}

/// Table row of a mineral, matching names case-insensitively.
fn mineral_row(table: &SignatureDb, name: &str) -> Option<usize> {
    let lower = name.to_lowercase();
    let wanted = ALIASES
        .iter()
        .find(|(alias, _)| *alias == lower)
        .map_or(lower.clone(), |(_, target)| target.to_lowercase());
    table
        .sources
        .iter()
        .position(|s| s.category == Category::ShipMining && s.name.to_lowercase() == wanted)
}

/// Add a ship-mining row after the last row of the same or a rarer tier
/// (the table runs from legendary to common), so it stays grouped by tier.
fn add_mineral(table: &mut SignatureDb, name: &str, tier: &str) -> usize {
    let rank = |t: &str| TIERS.iter().position(|x| *x == t);
    let new_rank = rank(tier);
    let at = table
        .sources
        .iter()
        .rposition(|s| {
            s.category == Category::ShipMining && s.tier.as_deref().and_then(rank) >= new_rank
        })
        .map_or(0, |i| i + 1);
    table.sources.insert(
        at,
        Source {
            name: name.to_owned(),
            category: Category::ShipMining,
            tier: Some(tier.to_owned()),
            base: 0,
            max_count: NEW_MINERAL_MAX_COUNT,
        },
    );
    at
}

/// Update `table` from extracted entities. `sc_version` replaces the
/// table's game version when given; `data_version` is stamped on the result
/// only when something changed.
pub fn merge(
    table: &SignatureDb,
    entities: &[EntitySignature],
    sc_version: Option<&str>,
    data_version: &str,
) -> Merge {
    let mut out = table.clone();
    let mut added = Vec::new();
    let mut votes: BTreeMap<String, BTreeMap<u32, Vec<&str>>> = BTreeMap::new();
    let mut unclaimed: BTreeMap<u32, Vec<&str>> = BTreeMap::new();
    for e in entities {
        // Zero means the entity never shows on the scanner.
        let Some(value) = e.cross_section().filter(|&v| v > 0) else {
            continue;
        };
        let row = match classify(&e.name) {
            Some(Kind::Row(name)) => name.to_owned(),
            Some(Kind::Mineral { name, tier }) => {
                let i = mineral_row(&out, &name).unwrap_or_else(|| {
                    added.push(name.clone());
                    add_mineral(&mut out, &name, tier)
                });
                out.sources[i].name.clone()
            }
            Some(Kind::Unknown) => {
                unclaimed.entry(value).or_default().push(&e.name);
                continue;
            }
            None => continue,
        };
        votes
            .entry(row)
            .or_default()
            .entry(value)
            .or_default()
            .push(&e.name);
    }

    let mut merge = Merge {
        table: table.clone(),
        changes: Vec::new(),
        added,
        confirmed: Vec::new(),
        unmatched_rows: Vec::new(),
        conflicts: Vec::new(),
        unclaimed: groups(unclaimed),
        changed: false,
    };
    for source in &mut out.sources {
        let groups = groups(votes.remove(&source.name).unwrap_or_default());
        let Some(top) = groups.first() else {
            merge.unmatched_rows.push(source.name.clone());
            continue;
        };
        if source.base == 0 {
            // A row added above takes its value as is.
        } else if top.value == source.base {
            merge.confirmed.push((source.name.clone(), top.count));
        } else {
            merge.changes.push(Change {
                name: source.name.clone(),
                old: source.base,
                new: top.value,
            });
        }
        source.base = top.value;
        if groups.len() > 1 {
            merge.conflicts.push((source.name.clone(), groups));
        }
    }
    if let Some(v) = sc_version.filter(|v| *v != out.sc_version) {
        out.sc_version = v.to_owned();
        merge.changed = true;
    }
    merge.changed |= !merge.changes.is_empty() || !merge.added.is_empty();
    if merge.changed {
        out.data_version = data_version.to_owned();
    }
    merge.table = out;
    merge
}

/// Value groups, most common first, ties by value.
fn groups(votes: BTreeMap<u32, Vec<&str>>) -> Vec<Group> {
    let mut out: Vec<Group> = votes
        .into_iter()
        .map(|(value, names)| Group {
            value,
            count: names.len(),
            examples: names.iter().take(3).map(|n| (*n).to_owned()).collect(),
        })
        .collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.value.cmp(&b.value)));
    out
}

/// Table as written to `data/signatures.json`.
pub fn to_json(table: &SignatureDb) -> String {
    let mut text = serde_json::to_string_pretty(table).expect("table serializes");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> SignatureDb {
        SignatureDb::from_json(include_str!("../../../data/signatures.json")).unwrap()
    }

    fn entity(name: &str, value: f32) -> EntitySignature {
        EntitySignature {
            name: name.into(),
            file: format!("libs/foundry/records/entities/mineable/{name}.xml"),
            channels: vec![0.0, 0.0, 0.0, 0.0, value],
        }
    }

    fn mineral(name: &str, tier: &'static str) -> Option<Kind> {
        Some(Kind::Mineral {
            name: name.into(),
            tier,
        })
    }

    #[test]
    fn classifies_entity_names() {
        assert_eq!(
            classify("MineableRock_AsteroidLegendary_Savrilium_RCD_large"),
            mineral("Savrilium", "legendary")
        );
        assert_eq!(
            classify("MineableRock_SurfaceCommon_Aluminum"),
            mineral("Aluminum", "common")
        );
        assert_eq!(
            classify("MineableRock_FPS_Carinite_small"),
            Some(Kind::Row(SMALL_DEPOSIT))
        );
        assert_eq!(
            classify("MineableRock_GroundVehicle_Beradom"),
            Some(Kind::Row(LARGE_DEPOSIT))
        );
        for panel in [
            "SalvageScrap_GenericPieces_MetalBeam",
            "SalvageLock_FPSSalvage",
            "SalvageableRepairable_ShipDebris_M_Jav_Panel_a",
        ] {
            assert_eq!(classify(panel), Some(Kind::Row(SALVAGE_PANELS)), "{panel}");
        }
        assert_eq!(
            classify("SalvageableDebris_C2"),
            Some(Kind::Row("Large Wreck Debris"))
        );
        assert_eq!(classify("SalvageableDebris_Redeemer"), Some(Kind::Unknown));
        assert_eq!(
            classify("MineableRock_AsteroidMythic_Unobtainium"),
            Some(Kind::Unknown)
        );
        // Leftovers, tests and templates, and things that aren't rocks.
        for ignored in [
            "AsteroidCTypeMineableRock_Copper",
            "GraniteMineableRock_Iron",
            "MineableRock_test_Gold",
            "MineableRock_SurfaceRare_TEMPLATE",
            "MineableRock_FPS_TEMPLATE",
            "AEGS_Avenger",
            "SalvageableDebris",
        ] {
            assert_eq!(classify(ignored), None, "{ignored}");
        }
    }

    #[test]
    fn takes_the_most_common_value_per_row() {
        let entities = [
            entity("MineableRock_AsteroidRare_Gold", 3590.0),
            entity("MineableRock_SurfaceRare_Gold", 3590.0),
            entity("MineableRock_SurfaceRare_Gold_RCD_small", 3585.0),
            entity("MineableRock_AsteroidCommon_Aluminium", 4285.0),
            entity("MineableRock_FPS_Hadanite", 0.0),
            entity("SalvageableDebris_Redeemer", 2000.0),
            entity("AEGS_Avenger", 1200.0),
        ];
        let m = merge(&table(), &entities, None, "2099.01.01");
        assert!(m.changed);
        assert_eq!(
            m.changes,
            vec![Change {
                name: "Gold".into(),
                old: 3585,
                new: 3590
            }]
        );
        assert_eq!(m.confirmed, vec![("Aluminum".to_string(), 1)]);
        assert_eq!(m.conflicts.len(), 1);
        assert_eq!(m.conflicts[0].1[0].count, 2);
        assert!(m.unmatched_rows.contains(&SMALL_DEPOSIT.to_string()));
        assert_eq!(
            m.unclaimed,
            vec![Group {
                value: 2000,
                count: 1,
                examples: vec!["SalvageableDebris_Redeemer".into()]
            }]
        );
        assert_eq!(m.table.data_version, "2099.01.01");
        let gold = m.table.sources.iter().find(|s| s.name == "Gold").unwrap();
        assert_eq!(gold.base, 3590);
        // The written file is a valid table again.
        SignatureDb::from_json(&to_json(&m.table)).unwrap();
    }

    #[test]
    fn adds_new_minerals_within_their_tier() {
        let entities = [
            entity("MineableRock_AsteroidEpic_Zenithite", 3415.0),
            entity("MineableRock_SurfaceEpic_Zenithite", 3415.0),
            entity("MineableRock_AsteroidCommon_Slag", 4315.0),
        ];
        let m = merge(&table(), &entities, None, "2099.01.01");
        assert!(m.changed);
        assert_eq!(m.added, vec!["Zenithite".to_string(), "Slag".to_string()]);
        let names: Vec<&str> = m.table.sources.iter().map(|s| s.name.as_str()).collect();
        let at = |n: &str| names.iter().position(|x| *x == n).unwrap();
        assert_eq!(at("Zenithite"), at("Lindinium") + 1);
        assert_eq!(at("Slag"), at("Ice") + 1);
        let z = &m.table.sources[at("Zenithite")];
        assert_eq!(
            (z.base, z.tier.as_deref(), z.max_count),
            (3415, Some("epic"), 100)
        );
        SignatureDb::from_json(&to_json(&m.table)).unwrap();

        let mut empty = table();
        empty.sources.clear();
        assert_eq!(add_mineral(&mut empty, "First", "rare"), 0);
    }

    #[test]
    fn leaves_the_version_alone_when_nothing_changed() {
        let t = table();
        let iron = [entity("MineableRock_AsteroidCommon_Iron", 4270.0)];
        let m = merge(&t, &iron, Some(&t.sc_version), "2099.01.01");
        assert!(!m.changed);
        assert_eq!(m.table.data_version, t.data_version);

        let m = merge(&t, &[], Some("4.8.0"), "2099.01.01");
        assert!(m.changed);
        assert_eq!(m.table.sc_version, "4.8.0");
        assert_eq!(m.table.data_version, "2099.01.01");
    }

    #[test]
    fn json_matches_the_checked_in_file() {
        let text = include_str!("../../../data/signatures.json");
        assert_eq!(to_json(&SignatureDb::from_json(text).unwrap()), text);
    }
}
