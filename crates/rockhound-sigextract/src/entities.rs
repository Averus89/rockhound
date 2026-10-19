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
        let Some(channels) = signature_channels(db, &db.fields(rec.ptr)?)? else {
            continue;
        };
        scan.with_signature += 1;
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

fn signature_channels(
    db: &DataCore,
    entity: &[(String, Value)],
) -> Result<Option<Vec<f32>>, DataCoreError> {
    let Some(Value::Array(components)) = get(entity, "Components") else {
        return Ok(None);
    };
    for component in components {
        let Value::Ptr(ptr) = component else { continue };
        if !db.is_a(ptr.struct_index, SIGNATURE_PARAMS) {
            continue;
        }
        let mut node = db.fields(*ptr)?;
        for step in CHAIN {
            match child(db, &node, step)? {
                Some(next) => node = next,
                None => return Ok(None),
            }
        }
        let Some(Value::Array(items)) = get(&node, SIGNATURES) else {
            return Ok(None);
        };
        let channels = items
            .iter()
            .map(|v| number(db, v))
            .collect::<Result<Option<Vec<f32>>, _>>()?;
        return Ok(channels);
    }
    Ok(None)
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
                with_signature: 2
            }
        );
        assert_eq!(found[0].name, "Alpha");
        assert_eq!(found[0].file, "a.xml");
        assert_eq!(found[0].cross_section(), Some(3170));
        assert_eq!(found[1].cross_section(), Some(4000));
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
