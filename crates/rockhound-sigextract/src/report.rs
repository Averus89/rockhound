//! Human-readable output of the extractor.

use std::fmt::Write;

use crate::entities::EntitySignature;
use crate::table::{Group, Merge};

/// What an update found and changed.
pub fn merge_report(m: &Merge) -> String {
    let mut out = String::new();
    let mut line = |s: String| writeln!(out, "{s}").unwrap();
    if m.changes.is_empty() {
        line("No signature values changed.".into());
    } else {
        line("Changed:".into());
        for c in &m.changes {
            line(format!("  {:<22} {} -> {}", c.name, c.old, c.new));
        }
    }
    if !m.confirmed.is_empty() {
        line(format!("Confirmed unchanged: {} rows", m.confirmed.len()));
    }
    if !m.unmatched_rows.is_empty() {
        line("No entity matched these rows (kept as they were):".into());
        for name in &m.unmatched_rows {
            line(format!("  {name}"));
        }
    }
    if !m.conflicts.is_empty() {
        line("Rows whose entities disagree (most common value used):".into());
        for (name, groups) in &m.conflicts {
            line(format!("  {name}: {}", group_list(groups)));
        }
    }
    if !m.unclaimed.is_empty() {
        line("Mining/salvage-looking entities no row claimed:".into());
        for g in &m.unclaimed {
            let known = m
                .table
                .sources
                .iter()
                .find(|s| s.base == g.value)
                .map(|s| format!(" (same as {})", s.name))
                .unwrap_or_default();
            line(format!(
                "  {:>6} x{:<4} {}{known}",
                g.value,
                g.count,
                g.examples.join(", ")
            ));
        }
    }
    out
}

fn group_list(groups: &[Group]) -> String {
    groups
        .iter()
        .map(|g| format!("{} (x{})", g.value, g.count))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Tab-separated listing: cross section, name, file, every channel.
pub fn entity_tsv(entities: &[EntitySignature], filter: Option<&str>) -> String {
    let filter = filter.map(str::to_lowercase);
    let mut out = String::from("cross_section\tname\tfile\tchannels\n");
    for e in entities {
        let haystack = format!("{} {}", e.name, e.file).to_lowercase();
        if filter.as_deref().is_some_and(|f| !haystack.contains(f)) {
            continue;
        }
        let cs = e.cross_section().map(|v| v.to_string()).unwrap_or_default();
        let channels: Vec<String> = e.channels.iter().map(|c| c.to_string()).collect();
        writeln!(out, "{cs}\t{}\t{}\t{}", e.name, e.file, channels.join(",")).unwrap();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::merge;
    use rockhound_core::SignatureDb;

    fn entity(name: &str, value: f32) -> EntitySignature {
        EntitySignature {
            name: name.into(),
            file: "libs/entities/mining.xml".into(),
            channels: vec![0.0, 0.0, 0.0, 0.0, value],
        }
    }

    #[test]
    fn reports_changes_conflicts_and_leftovers() {
        let table = SignatureDb::from_json(include_str!("../../../data/signatures.json")).unwrap();
        let m = merge(
            &table,
            &[
                entity("MineableRock_Gold_A", 3590.0),
                entity("MineableRock_Gold_B", 3585.0),
                entity("MineableRock_Gold_C", 3590.0),
                entity("Wreck_Small", 1700.0),
            ],
            None,
            "2099.01.01",
        );
        let text = merge_report(&m);
        assert!(
            text.contains("Gold                   3585 -> 3590"),
            "{text}"
        );
        assert!(text.contains("Gold: 3590 (x2), 3585 (x1)"), "{text}");
        assert!(text.contains("Quantainium"), "{text}");
        assert!(text.contains("(same as Small Wreck Debris)"), "{text}");

        let m = merge(&table, &[entity("MineableRock_Iron", 4270.0)], None, "x");
        let text = merge_report(&m);
        assert!(text.starts_with("No signature values changed."), "{text}");
        assert!(text.contains("Confirmed unchanged: 1 rows"), "{text}");
    }

    #[test]
    fn lists_entities_as_tsv() {
        let entities = [
            entity("MineableRock_Gold", 3585.0),
            entity("AEGS_Avenger", 1.0),
        ];
        let all = entity_tsv(&entities, None);
        assert_eq!(all.lines().count(), 3);
        assert!(all.contains("3585\tMineableRock_Gold\tlibs/entities/mining.xml\t0,0,0,0,3585"));
        let gold = entity_tsv(&entities, Some("GOLD"));
        assert_eq!(gold.lines().count(), 2);
    }
}
