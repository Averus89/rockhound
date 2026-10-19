//! Signature values of every entity in DataCore.
//!
//! The chain, as traced by Mallachi for SC 4.6/4.7:
//!
//! ```text
//! EntityClassDefinition.Components[]          (strong pointers)
//!   -> SSCSignatureSystemParams.radarProperties
//!     -> SSCRadarContactProperites.baseSignatureParams
//!       -> SSCSignatureSystemBaseSignatureParams.signatures[]   (floats)
//! ```
//!
//! The scanner shows channel 4 of `signatures`, the cross section.

use crate::datacore::{DataCore, DataCoreError, Value};

const ENTITY: &str = "EntityClassDefinition";
const SIGNATURE_PARAMS: &str = "SSCSignatureSystemParams";
const CHAIN: [&str; 2] = ["radarProperties", "baseSignatureParams"];
const SIGNATURES: &str = "signatures";
/// Index of the cross-section signature, the value the mining HUD shows.
pub const CROSS_SECTION: usize = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct EntitySignature {
    /// Record name without the `EntityClassDefinition.` prefix.
    pub name: String,
    /// Path of the record's source file in the game data.
    pub file: String,
    /// Every signature channel.
    pub channels: Vec<f32>,
}

impl EntitySignature {
    /// The value the scanner shows, rounded to a whole number.
    pub fn cross_section(&self) -> Option<u32> {
        let v = *self.channels.get(CROSS_SECTION)?;
        (v.is_finite() && v >= 0.0).then(|| v.round() as u32)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Scan {
    pub entities: usize,
    pub with_signature: usize,
    /// Signatures found by searching because the usual path didn't lead to
    /// them, a sign a patch moved them.
    pub searched: usize,
}

/// Every entity that has signature parameters, sorted by name.
pub fn entity_signatures(db: &DataCore) -> Result<(Vec<EntitySignature>, Scan), DataCoreError> {
    let mut out = Vec::new();
    let mut scan = Scan::default();
    for rec in db.records() {
        if !db.is_a(rec.ptr.struct_index, ENTITY) {
            continue;
        }
        scan.entities += 1;
        let Some((channels, searched)) = signature_channels(db, &db.fields(rec.ptr)?)? else {
            continue;
        };
        scan.with_signature += 1;
        scan.searched += usize::from(searched);
        let name = rec
            .name
            .strip_prefix(&format!("{ENTITY}."))
            .unwrap_or(&rec.name)
            .to_owned();
        out.push(EntitySignature {
            name,
            file: rec.file_name.into_owned(),
            channels,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((out, scan))
}

/// How deep the fallback search follows structs below the component; the
/// usual path is two steps.
const SEARCH_DEPTH: u32 = 4;

/// The entity's signature channels, and whether they had to be searched for.
fn signature_channels(
    db: &DataCore,
    entity: &[(String, Value)],
) -> Result<Option<(Vec<f32>, bool)>, DataCoreError> {
    let Some(Value::Array(components)) = get(entity, "Components") else {
        return Ok(None);
    };
    for component in components {
        let Value::Ptr(ptr) = component else { continue };
        // A renamed component class most likely keeps "SignatureSystem".
        if !db.is_a(ptr.struct_index, SIGNATURE_PARAMS)
            && !db.struct_name(ptr.struct_index).contains("SignatureSystem")
        {
            continue;
        }
        let params = db.fields(*ptr)?;
        if let Some(channels) = by_path(db, &params)? {
            return Ok(Some((channels, false)));
        }
        if let Some(channels) = search(db, &params, SEARCH_DEPTH)? {
            return Ok(Some((channels, true)));
        }
    }
    Ok(None)
}

/// Follow the known path to the signature array.
fn by_path(db: &DataCore, params: &[(String, Value)]) -> Result<Option<Vec<f32>>, DataCoreError> {
    let mut node = params.to_vec();
    for step in CHAIN {
        match child(db, &node, step)? {
            Some(next) => node = next,
            None => return Ok(None),
        }
    }
    // A short array here means the real one moved, so let the search run.
    Ok(channels(db, get(&node, SIGNATURES))?.filter(|c| c.len() > CROSS_SECTION))
}

/// Find the first `signatures` array long enough to hold the cross section,
/// anywhere below the component.
fn search(
    db: &DataCore,
    fields: &[(String, Value)],
    depth: u32,
) -> Result<Option<Vec<f32>>, DataCoreError> {
    for (name, value) in fields {
        if name == SIGNATURES {
            if let Some(c) = channels(db, Some(value))?.filter(|c| c.len() > CROSS_SECTION) {
                return Ok(Some(c));
            }
        }
    }
    if depth == 0 {
        return Ok(None);
    }
    for (name, _) in fields {
        if let Some(inner) = child(db, fields, name)? {
            if let Some(c) = search(db, &inner, depth - 1)? {
                return Ok(Some(c));
            }
        }
    }
    Ok(None)
}

fn channels(db: &DataCore, value: Option<&Value>) -> Result<Option<Vec<f32>>, DataCoreError> {
    let Some(Value::Array(items)) = value else {
        return Ok(None);
    };
    items
        .iter()
        .map(|v| number(db, v))
        .collect::<Result<Option<Vec<f32>>, _>>()
}

fn get<'a>(fields: &'a [(String, Value)], name: &str) -> Option<&'a Value> {
    fields.iter().find(|(n, _)| n == name).map(|(_, v)| v)
}

/// Follow a field into a pointed-to or inline struct.
fn child(
    db: &DataCore,
    fields: &[(String, Value)],
    name: &str,
) -> Result<Option<Vec<(String, Value)>>, DataCoreError> {
    Ok(match get(fields, name) {
        Some(Value::Ptr(p)) => Some(db.fields(*p)?),
        Some(Value::Class(inner)) => Some(inner.clone()),
        _ => None,
    })
}

/// A channel is a plain float, or a struct wrapping one.
fn number(db: &DataCore, v: &Value) -> Result<Option<f32>, DataCoreError> {
    let fields = match v {
        Value::Float(f) => return Ok(Some(*f as f32)),
        Value::Ptr(p) => db.fields(*p)?,
        Value::Class(fields) => fields.clone(),
        _ => return Ok(None),
    };
    Ok(fields.iter().find_map(|(_, v)| match v {
        Value::Float(f) => Some(*f as f32),
        _ => None,
    }))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::datacore::tests::*;
    use crate::datacore::Ptr;

    const NONE: u32 = u32::MAX;

    /// A DataCore with the signature chain and the given entities. Each
    /// entity is (record name, file, signature channels or None for an
    /// entity without signature parameters).
    pub fn game(entities: &[(&str, &str, Option<Vec<f32>>)]) -> Vec<u8> {
        let mut b = Builder::new();
        let entity = b.add_struct(S {
            name: ENTITY,
            parent: NONE,
            props: vec![array("Components", ty::STRONG, 0)],
        });
        let component = b.add_struct(S {
            name: "DataForgeComponentParams",
            parent: NONE,
            props: vec![],
        });
        let params = b.add_struct(S {
            name: SIGNATURE_PARAMS,
            parent: component,
            props: vec![
                prop("enabled", ty::BOOL),
                prop("radarProperties", ty::STRONG),
            ],
        });
        let radar = b.add_struct(S {
            name: "SSCRadarContactProperites",
            parent: NONE,
            props: vec![prop("baseSignatureParams", ty::STRONG)],
        });
        let base = b.add_struct(S {
            name: "SSCSignatureSystemBaseSignatureParams",
            parent: NONE,
            props: vec![array(SIGNATURES, ty::SINGLE, 0)],
        });
        let other = b.add_struct(S {
            name: "SGeometryResourceParams",
            parent: component,
            props: vec![],
        });
        let not_entity = b.add_struct(S {
            name: "AmmoParams",
            parent: NONE,
            props: vec![],
        });
        b.add_instance(other, vec![]);
        b.add_instance(not_entity, vec![]);
        b.add_record(
            "AmmoParams.Bullet",
            "ammo.xml",
            Ptr {
                struct_index: not_entity,
                instance: 0,
            },
        );

        for (name, file, channels) in entities {
            let first_strong = b.strongs.len() as u32;
            b.strongs.push((other, 0));
            if let Some(channels) = channels {
                let first_single = b.singles.len() as u32;
                b.singles.extend_from_slice(channels);
                let base_i = b.add_instance(
                    base,
                    [le32(channels.len() as u32), le32(first_single)].concat(),
                );
                let radar_i = b.add_instance(radar, ptr_bytes(base, base_i));
                let params_i =
                    b.add_instance(params, [vec![1], ptr_bytes(radar, radar_i)].concat());
                b.strongs.push((params, params_i));
            }
            let count = b.strongs.len() as u32 - first_strong;
            let instance = b.add_instance(entity, [le32(count), le32(first_strong)].concat());
            b.add_record(
                &format!("{ENTITY}.{name}"),
                file,
                Ptr {
                    struct_index: entity,
                    instance,
                },
            );
        }
        b.finish()
    }

    pub fn channels(cross_section: f32) -> Option<Vec<f32>> {
        Some(vec![0.0, 0.0, 0.0, 0.0, cross_section, 0.0, 0.0, 0.0])
    }

    #[test]
    fn follows_the_signature_chain() {
        let db = DataCore::parse(game(&[
            ("Zeta", "z.xml", channels(4000.0)),
            ("Alpha", "a.xml", channels(3170.4)),
            ("NoSignature", "n.xml", None),
        ]))
        .unwrap();
        let (found, scan) = entity_signatures(&db).unwrap();
        assert_eq!(
            scan,
            Scan {
                entities: 3,
                with_signature: 2,
                searched: 0,
            }
        );
        assert_eq!(found[0].name, "Alpha");
        assert_eq!(found[0].file, "a.xml");
        assert_eq!(found[0].cross_section(), Some(3170));
        assert_eq!(found[1].cross_section(), Some(4000));
    }

    /// A patch renames the component and its fields and nests the array
    /// one level deeper, inline.
    #[test]
    fn finds_moved_signatures_by_searching() {
        let mut b = Builder::new();
        let entity = b.add_struct(S {
            name: ENTITY,
            parent: NONE,
            props: vec![array("Components", ty::STRONG, 0)],
        });
        let holder = b.add_struct(S {
            name: "SignatureHolder",
            parent: NONE,
            props: vec![
                array("signatures", ty::SINGLE, 0),
                array("modifiers", ty::SINGLE, 0),
            ],
        });
        let detail = b.add_struct(S {
            name: "RadarDetail",
            parent: NONE,
            props: vec![
                prop("range", ty::SINGLE),
                P {
                    target: holder as u16,
                    ..prop("sig", ty::CLASS)
                },
            ],
        });
        let params = b.add_struct(S {
            name: "SSCSignatureSystemParamsV2",
            parent: NONE,
            props: vec![prop("radar", ty::STRONG)],
        });
        // A float and a second, shorter array on the way.
        b.singles = vec![9.0, 0.0, 0.0, 0.0, 0.0, 2400.0];
        let detail_i = b.add_instance(
            detail,
            [
                1.0f32.to_le_bytes().to_vec(),
                le32(6),
                le32(0),
                le32(1),
                le32(0),
            ]
            .concat(),
        );
        let params_i = b.add_instance(params, ptr_bytes(detail, detail_i));
        b.strongs.push((params, params_i));
        let e = b.add_instance(entity, [le32(1), le32(0)].concat());
        b.add_record(
            "EntityClassDefinition.SalvageableDebris_C2",
            "c2.xml",
            Ptr {
                struct_index: entity,
                instance: e,
            },
        );
        let db = DataCore::parse(b.finish()).unwrap();
        let (found, scan) = entity_signatures(&db).unwrap();
        assert_eq!(scan.searched, 1);
        assert_eq!(found[0].channels, vec![9.0, 0.0, 0.0, 0.0, 0.0, 2400.0]);
    }

    #[test]
    fn skips_short_arrays_at_the_known_path() {
        let db = DataCore::parse(game(&[("Short", "s.xml", Some(vec![1.0, 2.0]))])).unwrap();
        let (found, scan) = entity_signatures(&db).unwrap();
        assert!(found.is_empty());
        assert_eq!(scan.with_signature, 0);
    }

    #[test]
    fn cross_section_needs_a_valid_channel() {
        let mut e = EntitySignature {
            name: "x".into(),
            file: String::new(),
            channels: vec![1.0],
        };
        assert_eq!(e.cross_section(), None);
        e.channels = vec![0.0, 0.0, 0.0, 0.0, f32::NAN];
        assert_eq!(e.cross_section(), None);
    }

    #[test]
    fn reads_wrapped_channels_and_inline_structs() {
        let db = DataCore::parse(game(&[])).unwrap();
        let wrapped = Value::Class(vec![("value".into(), Value::Float(2.0))]);
        assert_eq!(number(&db, &wrapped).unwrap(), Some(2.0));
        assert_eq!(number(&db, &Value::Null).unwrap(), None);
        let fields = vec![("inner".to_string(), wrapped)];
        assert!(child(&db, &fields, "inner").unwrap().is_some());
        assert!(child(&db, &fields, "missing").unwrap().is_none());
    }
}
