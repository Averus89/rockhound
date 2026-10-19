//! Merge extracted entity signatures into the app's signature table.
//!
//! The game has hundreds of entities per table row (every rock size and
//! variant), so each row takes the most common value among the entities
//! that match it. Names, tiers and cluster limits are not in the game data
//! and are kept from the existing table.

use std::collections::BTreeMap;

use rockhound_core::signatures::Category;
use rockhound_core::SignatureDb;

use crate::entities::EntitySignature;

/// Game spellings that differ from the table's names.
const ALIASES: [(&str, &str); 4] = [
    ("aluminium", "Aluminum"),
    ("quantanium", "Quantainium"),
    ("sileron", "Stileron"),
    ("stileron", "Stileron"),
];
const SMALL_DEPOSIT: &str = "Small Ground Deposit";
const LARGE_DEPOSIT: &str = "Large Ground Deposit";
const SALVAGE_PANELS: &str = "Salvage Panels";
/// Words that mark an entity as something a mining or salvage scan finds,
/// for the report of entities no table row claimed.
const SCANNABLE: [&str; 8] = [
    "mineable", "asteroid", "mining", "deposit", "salvage", "wreck", "debris", "rock",
];

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
    /// Rows whose entities agree with the table.
    pub confirmed: Vec<(String, usize)>,
    /// Rows no entity matched, left as they were.
    pub unmatched_rows: Vec<String>,
    /// Rows whose entities disagree: every value seen, most common first.
    pub conflicts: Vec<(String, Vec<Group>)>,
    /// Scannable-looking entities no row claimed, by value.
    pub unclaimed: Vec<Group>,
    /// Whether anything in the table changed (values or game version).
    pub changed: bool,
}

/// Which table row an entity belongs to, if any.
pub fn row_for(entity: &EntitySignature, table: &SignatureDb) -> Option<usize> {
    let lower = entity.name.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let has = |w: &str| words.contains(&w);
    let row = |name: &str| table.sources.iter().position(|s| s.name == name);
    let mineable = lower.contains("mineable") || lower.contains("asteroid");

    if mineable && has("fps") {
        return row(SMALL_DEPOSIT);
    }
    if mineable && has("groundvehicle") {
        return row(LARGE_DEPOSIT);
    }
    if ["salvagescrap", "salvagelock", "salvagepanel"]
        .iter()
        .any(|w| lower.contains(w))
    {
        return row(SALVAGE_PANELS);
    }
    if !mineable || has("harvestable") {
        return None;
    }
    table.sources.iter().position(|s| {
        s.category == Category::ShipMining && {
            let name = s.name.to_lowercase();
            has(&name)
                || ALIASES
                    .iter()
                    .any(|(alias, target)| *target == s.name && has(alias))
        }
    })
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
    let mut votes: Vec<BTreeMap<u32, Vec<&str>>> = vec![BTreeMap::new(); table.sources.len()];
    let mut unclaimed: BTreeMap<u32, Vec<&str>> = BTreeMap::new();
    for e in entities {
        // Zero means the entity never shows on the scanner.
        let Some(value) = e.cross_section().filter(|&v| v > 0) else {
            continue;
        };
        match row_for(e, table) {
            Some(i) => votes[i].entry(value).or_default().push(&e.name),
            None if is_scannable(e) => unclaimed.entry(value).or_default().push(&e.name),
            None => {}
        }
    }

    let mut out = table.clone();
    let mut merge = Merge {
        table: table.clone(),
        changes: Vec::new(),
        confirmed: Vec::new(),
        unmatched_rows: Vec::new(),
        conflicts: Vec::new(),
        unclaimed: groups(unclaimed),
        changed: false,
    };
    for (source, votes) in out.sources.iter_mut().zip(votes) {
        let groups = groups(votes);
        let Some(top) = groups.first() else {
            merge.unmatched_rows.push(source.name.clone());
            continue;
        };
        if top.value == source.base {
            merge.confirmed.push((source.name.clone(), top.count));
        } else {
            merge.changes.push(Change {
                name: source.name.clone(),
                old: source.base,
                new: top.value,
            });
            source.base = top.value;
        }
        if groups.len() > 1 {
            merge.conflicts.push((source.name.clone(), groups));
        }
    }
    if let Some(v) = sc_version.filter(|v| *v != out.sc_version) {
        out.sc_version = v.to_owned();
        merge.changed = true;
    }
    merge.changed |= !merge.changes.is_empty();
    if merge.changed {
        out.data_version = data_version.to_owned();
    }
    merge.table = out;
    merge
}

fn is_scannable(e: &EntitySignature) -> bool {
    let text = format!("{} {}", e.name, e.file).to_lowercase();
    SCANNABLE.iter().any(|w| text.contains(w))
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
            file: format!("libs/foundry/records/entities/{name}.xml"),
            channels: vec![0.0, 0.0, 0.0, 0.0, value],
        }
    }

    fn row_name(name: &str) -> Option<String> {
        let t = table();
        row_for(&entity(name, 1.0), &t).map(|i| t.sources[i].name.clone())
    }

    #[test]
    fn maps_entity_names_to_rows() {
        assert_eq!(
            row_name("MineableRock_Asteroid_Quantanium_Large").as_deref(),
            Some("Quantainium")
        );
        assert_eq!(
            row_name("Asteroid_Aluminium_1").as_deref(),
            Some("Aluminum")
        );
        assert_eq!(row_name("MineableRock_Gold").as_deref(), Some("Gold"));
        assert_eq!(
            row_name("MineableRock_FPS_Hadanite").as_deref(),
            Some(SMALL_DEPOSIT)
        );
        assert_eq!(
            row_name("MineableRock_GroundVehicle_Gold").as_deref(),
            Some(LARGE_DEPOSIT)
        );
        assert_eq!(
            row_name("SalvageScrap_Panel_01").as_deref(),
            Some(SALVAGE_PANELS)
        );
        // Not rocks, or only a substring of a mineral name.
        assert_eq!(row_name("Harvestable_Mineable_Gold"), None);
        assert_eq!(row_name("AEGS_Gold_Edition"), None);
        assert_eq!(row_name("MineableRock_Goldish"), None);
    }

    #[test]
    fn takes_the_most_common_value_per_row() {
        let entities = [
            entity("MineableRock_Gold_A", 3590.0),
            entity("MineableRock_Gold_B", 3590.0),
            entity("MineableRock_Gold_C", 3585.0),
            entity("MineableRock_Iron", 4270.0),
            entity("MineableRock_FPS_Hadanite", 0.0),
            entity("Wreck_Debris_Small", 1700.0),
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
        assert_eq!(m.confirmed, vec![("Iron".to_string(), 1)]);
        assert_eq!(m.conflicts.len(), 1);
        assert_eq!(m.conflicts[0].1[0].count, 2);
        assert!(m.unmatched_rows.contains(&SMALL_DEPOSIT.to_string()));
        assert_eq!(
            m.unclaimed,
            vec![Group {
                value: 1700,
                count: 1,
                examples: vec!["Wreck_Debris_Small".into()]
            }]
        );
        assert_eq!(m.table.data_version, "2099.01.01");
        let gold = m.table.sources.iter().find(|s| s.name == "Gold").unwrap();
        assert_eq!(gold.base, 3590);
        // The written file is a valid table again.
        SignatureDb::from_json(&to_json(&m.table)).unwrap();
    }

    #[test]
    fn leaves_the_version_alone_when_nothing_changed() {
        let t = table();
        let m = merge(
            &t,
            &[entity("MineableRock_Iron", 4270.0)],
            Some(&t.sc_version),
            "2099.01.01",
        );
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
