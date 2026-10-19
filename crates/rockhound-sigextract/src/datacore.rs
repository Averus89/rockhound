//! Reader for DataCore (`Game2.dcb`), the binary database inside Data.p4k
//! that holds every game record, including entity signatures.
//!
//! Layout as documented by StarBreaker and Mallachi's original extractor
//! notes (SC_Signature_Scanner `SignatureValue.md`): a 120-byte header of
//! counts, definition tables (structs, properties, enums, data mappings,
//! records), typed value pools, two string tables, then the instance data of
//! every struct, one block per data mapping.

use std::borrow::Cow;

#[derive(Debug, thiserror::Error)]
pub enum DataCoreError {
    #[error("Game2.dcb is truncated or corrupt (read past the end at byte {0})")]
    Truncated(usize),
    #[error("unsupported Game2.dcb layout: {0}. The game may have changed its file format")]
    Layout(String),
    #[error("Game2.dcb points at a missing {0}")]
    BadIndex(&'static str),
}

type Result<T> = std::result::Result<T, DataCoreError>;

const HEADER_LEN: usize = 120;
const NONE: u32 = u32::MAX;

pub(crate) mod ty {
    pub const BOOL: u16 = 0x01;
    pub const I8: u16 = 0x02;
    pub const I16: u16 = 0x03;
    pub const I32: u16 = 0x04;
    pub const I64: u16 = 0x05;
    pub const U8: u16 = 0x06;
    pub const U16: u16 = 0x07;
    pub const U32: u16 = 0x08;
    pub const U64: u16 = 0x09;
    pub const STRING: u16 = 0x0a;
    pub const SINGLE: u16 = 0x0b;
    pub const DOUBLE: u16 = 0x0c;
    pub const LOCALE: u16 = 0x0d;
    pub const GUID: u16 = 0x0e;
    pub const ENUM: u16 = 0x0f;
    pub const CLASS: u16 = 0x10;
    pub const STRONG: u16 = 0x110;
    pub const WEAK: u16 = 0x210;
    pub const REFERENCE: u16 = 0x310;
}

/// Value pools in file order, which differs from the header's count order
/// (booleans come after the unsigned integers). The type list doubles as the
/// element type of each pool; the last entry holds enum option names.
const POOLS: [u16; 19] = [
    ty::I8,
    ty::I16,
    ty::I32,
    ty::I64,
    ty::U8,
    ty::U16,
    ty::U32,
    ty::U64,
    ty::BOOL,
    ty::SINGLE,
    ty::DOUBLE,
    ty::GUID,
    ty::STRING,
    ty::LOCALE,
    ty::ENUM,
    ty::STRONG,
    ty::WEAK,
    ty::REFERENCE,
    ENUM_OPTION_POOL,
];
const ENUM_OPTION_POOL: u16 = 0xffff;
/// Header slot of each pool's count, in the order of [`POOLS`].
const POOL_COUNT_SLOT: [usize; 19] = [
    10, 11, 12, 13, 14, 15, 16, 17, 9, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27,
];

fn size_of(data_type: u16) -> Option<usize> {
    Some(match data_type {
        ty::BOOL | ty::I8 | ty::U8 => 1,
        ty::I16 | ty::U16 => 2,
        ty::I32 | ty::U32 | ty::SINGLE | ty::STRING | ty::LOCALE | ty::ENUM => 4,
        ENUM_OPTION_POOL => 4,
        ty::I64 | ty::U64 | ty::DOUBLE | ty::STRONG | ty::WEAK => 8,
        ty::GUID => 16,
        ty::REFERENCE => 20,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy)]
struct StructDef {
    name: u32,
    parent: u32,
    prop_count: u16,
    first_prop: u16,
    size: u32,
}

#[derive(Debug, Clone, Copy)]
struct PropDef {
    name: u32,
    struct_index: u16,
    data_type: u16,
    is_array: bool,
}

#[derive(Debug, Clone, Copy)]
struct RecordDef {
    name: u32,
    file_name: u32,
    struct_index: u32,
    instance: u16,
}

/// A struct instance in the data section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ptr {
    pub struct_index: u32,
    pub instance: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    Str(String),
    Guid(String),
    /// A pointer, or an element of a class array, both of which live in the
    /// data section.
    Ptr(Ptr),
    /// A reference to another record by id.
    Reference(String),
    /// An inline class, decoded in place.
    Class(Vec<(String, Value)>),
    Array(Vec<Value>),
}

/// A top-level record such as one entity.
#[derive(Debug, Clone)]
pub struct Record<'a> {
    pub name: Cow<'a, str>,
    pub file_name: Cow<'a, str>,
    pub struct_name: Cow<'a, str>,
    pub ptr: Ptr,
}

pub struct DataCore {
    bytes: Vec<u8>,
    structs: Vec<StructDef>,
    props: Vec<PropDef>,
    enum_count: usize,
    records: Vec<RecordDef>,
    /// Start of each pool, in the order of [`POOLS`].
    pools: [usize; 19],
    pool_counts: [usize; 19],
    /// Values, file names and string properties.
    text1: (usize, usize),
    /// Struct, property, enum and record names.
    text2: (usize, usize),
    /// Data section offset and instance count per struct.
    blocks: Vec<Option<(usize, u32)>>,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.bytes.len());
        let end = end.ok_or(DataCoreError::Truncated(self.pos))?;
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
}

impl DataCore {
    pub fn parse(bytes: Vec<u8>) -> Result<Self> {
        let mut c = Cursor {
            bytes: &bytes,
            pos: 0,
        };
        let header: Vec<usize> = (0..HEADER_LEN / 4)
            .map(|_| c.u32().map(|v| v as usize))
            .collect::<Result<_>>()?;
        let version = header[1];
        if version < 6 {
            return Err(DataCoreError::Layout(format!(
                "file version {version}, expected 6 or newer"
            )));
        }
        let [struct_count, prop_count, enum_count, mapping_count, record_count] =
            [header[4], header[5], header[6], header[7], header[8]];

        let structs = (0..struct_count)
            .map(|_| {
                Ok(StructDef {
                    name: c.u32()?,
                    parent: c.u32()?,
                    prop_count: c.u16()?,
                    first_prop: c.u16()?,
                    size: c.u32()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let props = (0..prop_count)
            .map(|_| {
                let p = PropDef {
                    name: c.u32()?,
                    struct_index: c.u16()?,
                    data_type: c.u16()?,
                    is_array: c.u16()? != 0,
                };
                c.take(2)?;
                Ok(p)
            })
            .collect::<Result<Vec<_>>>()?;
        c.take(enum_count * 8)?;
        let mappings = (0..mapping_count)
            .map(|_| Ok((c.u32()?, c.u32()?)))
            .collect::<Result<Vec<_>>>()?;
        let records = (0..record_count)
            .map(|_| {
                let name = c.u32()?;
                let file_name = c.u32()?;
                // Version 8 (SC 4.x) adds a string offset after the file name.
                if version >= 8 {
                    c.take(4)?;
                }
                let struct_index = c.u32()?;
                c.take(16)?;
                let instance = c.u16()?;
                c.take(2)?;
                Ok(RecordDef {
                    name,
                    file_name,
                    struct_index,
                    instance,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let mut pools = [0; 19];
        let mut pool_counts = [0; 19];
        for (i, &t) in POOLS.iter().enumerate() {
            pools[i] = c.pos;
            pool_counts[i] = header[POOL_COUNT_SLOT[i]];
            c.take(pool_counts[i] * size_of(t).unwrap())?;
        }
        let text1 = (c.pos, c.pos + header[28]);
        c.take(header[28])?;
        let text2 = (c.pos, c.pos + header[29]);
        c.take(header[29])?;

        let mut blocks = vec![None; structs.len()];
        let mut pos = c.pos;
        for (count, struct_index) in mappings {
            let def = structs
                .get(struct_index as usize)
                .ok_or(DataCoreError::BadIndex("struct in a data mapping"))?;
            let slot = &mut blocks[struct_index as usize];
            if slot.is_none() {
                *slot = Some((pos, count));
            }
            pos += count as usize * def.size as usize;
        }
        if pos != bytes.len() {
            // Name the counts, so a report of this error says which table
            // changed size (the gap is usually a multiple of one count).
            return Err(DataCoreError::Layout(format!(
                "data section ends at byte {pos} but the file has {} bytes \
                 (version {version}, structs {struct_count}, properties {prop_count}, \
                 enums {enum_count}, mappings {mapping_count}, records {record_count}, \
                 header {:?})",
                bytes.len(),
                &header[9..]
            )));
        }

        let db = Self {
            bytes,
            structs,
            props,
            enum_count,
            records,
            pools,
            pool_counts,
            text1,
            text2,
            blocks,
        };
        db.check_struct_sizes()?;
        Ok(db)
    }

    /// Every struct's fields must add up to its declared size, or the
    /// property encoding has changed and nothing read after it is right.
    fn check_struct_sizes(&self) -> Result<()> {
        for i in 0..self.structs.len() {
            let computed = self.inline_size(i as u32)?;
            let declared = self.structs[i].size as usize;
            if computed != declared {
                return Err(DataCoreError::Layout(format!(
                    "struct {} fields take {computed} bytes, file says {declared}",
                    self.struct_name(i as u32)
                )));
            }
        }
        Ok(())
    }

    fn inline_size(&self, struct_index: u32) -> Result<usize> {
        let mut total = 0;
        for p in self.props_of(struct_index)? {
            total += if p.is_array {
                8
            } else if p.data_type == ty::CLASS {
                self.inline_size(u32::from(p.struct_index))?
            } else {
                size_of(p.data_type).ok_or_else(|| {
                    DataCoreError::Layout(format!("unknown data type {:#x}", p.data_type))
                })?
            };
        }
        Ok(total)
    }

    pub fn struct_count(&self) -> usize {
        self.structs.len()
    }

    pub fn enum_count(&self) -> usize {
        self.enum_count
    }

    pub fn struct_name(&self, struct_index: u32) -> Cow<'_, str> {
        match self.structs.get(struct_index as usize) {
            Some(s) => self.text(self.text2, s.name),
            None => Cow::Borrowed("?"),
        }
    }

    /// True when the struct is `name` or inherits from it.
    pub fn is_a(&self, mut struct_index: u32, name: &str) -> bool {
        while let Some(s) = self.structs.get(struct_index as usize) {
            if self.text(self.text2, s.name) == name {
                return true;
            }
            struct_index = s.parent;
        }
        false
    }

    pub fn records(&self) -> impl Iterator<Item = Record<'_>> + '_ {
        self.records.iter().map(|r| Record {
            name: self.text(self.text2, r.name),
            file_name: self.text(self.text1, r.file_name),
            struct_name: self.struct_name(r.struct_index),
            ptr: Ptr {
                struct_index: r.struct_index,
                instance: u32::from(r.instance),
            },
        })
    }

    /// Decode the fields of one instance. Pointers are left unfollowed.
    pub fn fields(&self, ptr: Ptr) -> Result<Vec<(String, Value)>> {
        let (start, count) = self
            .blocks
            .get(ptr.struct_index as usize)
            .copied()
            .flatten()
            .ok_or(DataCoreError::BadIndex("struct instance block"))?;
        if ptr.instance >= count {
            return Err(DataCoreError::BadIndex("struct instance"));
        }
        let size = self.structs[ptr.struct_index as usize].size as usize;
        let mut c = Cursor {
            bytes: &self.bytes,
            pos: start + ptr.instance as usize * size,
        };
        self.read_struct(ptr.struct_index, &mut c)
    }

    /// One field of an instance by name.
    pub fn field(&self, ptr: Ptr, name: &str) -> Result<Option<Value>> {
        Ok(self
            .fields(ptr)?
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v))
    }

    /// Properties of a struct, inherited ones first, in storage order.
    fn props_of(&self, struct_index: u32) -> Result<Vec<PropDef>> {
        let mut chain = Vec::new();
        let mut i = struct_index;
        while i != NONE {
            let s = self
                .structs
                .get(i as usize)
                .ok_or(DataCoreError::BadIndex("parent struct"))?;
            chain.push(*s);
            if chain.len() > self.structs.len() {
                return Err(DataCoreError::Layout("struct inherits from itself".into()));
            }
            i = s.parent;
        }
        let mut out = Vec::new();
        for s in chain.iter().rev() {
            let first = usize::from(s.first_prop);
            let props = self
                .props
                .get(first..first + usize::from(s.prop_count))
                .ok_or(DataCoreError::BadIndex("property"))?;
            out.extend_from_slice(props);
        }
        Ok(out)
    }

    fn read_struct(&self, struct_index: u32, c: &mut Cursor) -> Result<Vec<(String, Value)>> {
        self.props_of(struct_index)?
            .into_iter()
            .map(|p| {
                let name = self.text(self.text2, p.name).into_owned();
                let value = if p.is_array {
                    let count = c.u32()?;
                    let first = c.u32()?;
                    Value::Array(
                        (first..first.saturating_add(count))
                            .map(|i| self.pool_value(&p, i))
                            .collect::<Result<_>>()?,
                    )
                } else if p.data_type == ty::CLASS {
                    Value::Class(self.read_struct(u32::from(p.struct_index), c)?)
                } else {
                    self.scalar(p.data_type, c)?
                };
                Ok((name, value))
            })
            .collect()
    }

    fn pool_value(&self, p: &PropDef, index: u32) -> Result<Value> {
        if p.data_type == ty::CLASS {
            return Ok(Value::Ptr(Ptr {
                struct_index: u32::from(p.struct_index),
                instance: index,
            }));
        }
        let pool = POOLS
            .iter()
            .position(|&t| t == p.data_type)
            .ok_or_else(|| DataCoreError::Layout(format!("no pool for type {:#x}", p.data_type)))?;
        if index as usize >= self.pool_counts[pool] {
            return Err(DataCoreError::BadIndex("array element"));
        }
        let mut c = Cursor {
            bytes: &self.bytes,
            pos: self.pools[pool] + index as usize * size_of(p.data_type).unwrap(),
        };
        self.scalar(p.data_type, &mut c)
    }

    fn scalar(&self, data_type: u16, c: &mut Cursor) -> Result<Value> {
        let size = size_of(data_type)
            .ok_or_else(|| DataCoreError::Layout(format!("unknown data type {data_type:#x}")))?;
        let b = c.take(size)?;
        let u = |n: usize| {
            let mut buf = [0u8; 8];
            buf[..n].copy_from_slice(&b[..n]);
            u64::from_le_bytes(buf)
        };
        Ok(match data_type {
            ty::BOOL => Value::Bool(b[0] != 0),
            ty::I8 => Value::Int(i64::from(b[0] as i8)),
            ty::I16 => Value::Int(i64::from(u(2) as u16 as i16)),
            ty::I32 => Value::Int(i64::from(u(4) as u32 as i32)),
            ty::I64 => Value::Int(u(8) as i64),
            ty::U8 | ty::U16 | ty::U32 | ty::U64 => Value::UInt(u(size)),
            ty::SINGLE => Value::Float(f64::from(f32::from_bits(u(4) as u32))),
            ty::DOUBLE => Value::Float(f64::from_bits(u(8))),
            ty::STRING | ty::LOCALE | ty::ENUM => {
                Value::Str(self.text(self.text1, u(4) as u32).into_owned())
            }
            ty::GUID => Value::Guid(guid(b)),
            ty::REFERENCE => Value::Reference(guid(&b[4..])),
            _ => {
                let struct_index = u(4) as u32;
                let instance = (u(8) >> 32) as u32;
                if struct_index == NONE || instance == NONE {
                    Value::Null
                } else {
                    Value::Ptr(Ptr {
                        struct_index,
                        instance,
                    })
                }
            }
        })
    }

    fn text(&self, table: (usize, usize), offset: u32) -> Cow<'_, str> {
        let start = table.0 + offset as usize;
        if start >= table.1 {
            return Cow::Borrowed("?");
        }
        let slice = &self.bytes[start..table.1];
        let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
        String::from_utf8_lossy(&slice[..end])
    }

    /// Instance as JSON, following pointers `depth` levels deep.
    pub fn to_json(&self, ptr: Ptr, depth: u32) -> Result<serde_json::Value> {
        let fields = self.fields(ptr)?;
        let mut map = serde_json::Map::new();
        map.insert(
            "_type".into(),
            self.struct_name(ptr.struct_index).into_owned().into(),
        );
        for (name, value) in fields {
            map.insert(name, self.value_json(&value, depth)?);
        }
        Ok(map.into())
    }

    fn value_json(&self, value: &Value, depth: u32) -> Result<serde_json::Value> {
        use serde_json::Value as J;
        Ok(match value {
            Value::Null => J::Null,
            Value::Bool(b) => J::Bool(*b),
            Value::Int(i) => (*i).into(),
            Value::UInt(u) => (*u).into(),
            Value::Float(f) => serde_json::Number::from_f64(*f).map_or(J::Null, J::Number),
            Value::Str(s) => J::String(s.clone()),
            Value::Guid(g) => J::String(g.clone()),
            Value::Reference(g) => J::String(format!("ref:{g}")),
            Value::Ptr(p) if depth > 0 => self.to_json(*p, depth - 1)?,
            Value::Ptr(p) => J::String(format!(
                "{}[{}]",
                self.struct_name(p.struct_index),
                p.instance
            )),
            Value::Class(fields) => {
                let mut map = serde_json::Map::new();
                for (name, v) in fields {
                    map.insert(name.clone(), self.value_json(v, depth)?);
                }
                J::Object(map)
            }
            Value::Array(items) => J::Array(
                items
                    .iter()
                    .map(|v| self.value_json(v, depth))
                    .collect::<Result<_>>()?,
            ),
        })
    }
}

/// Format 16 GUID bytes the way .NET prints them.
fn guid(b: &[u8]) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{}",
        u32::from_le_bytes(b[0..4].try_into().unwrap()),
        u16::from_le_bytes([b[4], b[5]]),
        u16::from_le_bytes([b[6], b[7]]),
        b[8],
        b[9],
        b[10..16]
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect::<String>()
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) use super::ty;

    /// Property of a test struct.
    pub struct P {
        pub name: &'static str,
        pub data_type: u16,
        pub array: bool,
        /// Struct index for class and pointer properties.
        pub target: u16,
    }

    pub fn prop(name: &'static str, data_type: u16) -> P {
        P {
            name,
            data_type,
            array: false,
            target: 0,
        }
    }

    pub fn array(name: &'static str, data_type: u16, target: u16) -> P {
        P {
            name,
            data_type,
            array: true,
            target,
        }
    }

    pub struct S {
        pub name: &'static str,
        pub parent: u32,
        pub props: Vec<P>,
    }

    /// Builds a Game2.dcb in memory: define structs, add instances as raw
    /// bytes, fill pools, then `finish`.
    #[derive(Default)]
    pub struct Builder {
        pub structs: Vec<S>,
        pub instances: Vec<Vec<Vec<u8>>>,
        pub records: Vec<(String, String, u32, u16)>,
        pub singles: Vec<f32>,
        pub strongs: Vec<(u32, u32)>,
        pub int32s: Vec<i32>,
        pub text1: Vec<u8>,
        pub text2: Vec<u8>,
        pub version: u32,
    }

    impl Builder {
        pub fn new() -> Self {
            Self {
                version: 6,
                // Offset 0 of each table is the empty string.
                text1: vec![0],
                text2: vec![0],
                ..Default::default()
            }
        }

        pub fn add_struct(&mut self, s: S) -> u32 {
            self.structs.push(s);
            self.instances.push(Vec::new());
            (self.structs.len() - 1) as u32
        }

        pub fn add_instance(&mut self, struct_index: u32, bytes: Vec<u8>) -> u32 {
            let list = &mut self.instances[struct_index as usize];
            list.push(bytes);
            (list.len() - 1) as u32
        }

        pub fn add_record(&mut self, name: &str, file: &str, ptr: Ptr) {
            self.records.push((
                name.into(),
                file.into(),
                ptr.struct_index,
                ptr.instance as u16,
            ));
        }

        pub fn text1(&mut self, s: &str) -> u32 {
            intern(&mut self.text1, s)
        }

        fn size(&self, struct_index: usize) -> usize {
            let s = &self.structs[struct_index];
            let own: usize = s
                .props
                .iter()
                .map(|p| {
                    if p.array {
                        8
                    } else if p.data_type == ty::CLASS {
                        self.size(usize::from(p.target))
                    } else {
                        size_of(p.data_type).unwrap()
                    }
                })
                .sum();
            let parent = if s.parent == NONE {
                0
            } else {
                self.size(s.parent as usize)
            };
            own + parent
        }

        pub fn finish(mut self) -> Vec<u8> {
            let mut structs = Vec::new();
            let mut props = Vec::new();
            let mut prop_count = 0u16;
            let sizes: Vec<usize> = (0..self.structs.len()).map(|i| self.size(i)).collect();
            for (i, s) in self.structs.iter().enumerate() {
                let name = intern(&mut self.text2, s.name);
                structs.extend_from_slice(&name.to_le_bytes());
                structs.extend_from_slice(&s.parent.to_le_bytes());
                structs.extend_from_slice(&(s.props.len() as u16).to_le_bytes());
                structs.extend_from_slice(&prop_count.to_le_bytes());
                structs.extend_from_slice(&(sizes[i] as u32).to_le_bytes());
                for p in &s.props {
                    let name = intern(&mut self.text2, p.name);
                    props.extend_from_slice(&name.to_le_bytes());
                    props.extend_from_slice(&p.target.to_le_bytes());
                    props.extend_from_slice(&p.data_type.to_le_bytes());
                    props.extend_from_slice(&u16::from(p.array).to_le_bytes());
                    props.extend_from_slice(&[0, 0]);
                    prop_count += 1;
                }
            }
            let mut records = Vec::new();
            let record_count = self.records.len() as u32;
            for (name, file, struct_index, instance) in std::mem::take(&mut self.records) {
                let name = intern(&mut self.text2, &name);
                let file = intern(&mut self.text1, &file);
                records.extend_from_slice(&name.to_le_bytes());
                records.extend_from_slice(&file.to_le_bytes());
                if self.version >= 8 {
                    records.extend_from_slice(&[0xcd; 4]);
                }
                records.extend_from_slice(&struct_index.to_le_bytes());
                records.extend_from_slice(&[0xab; 16]);
                records.extend_from_slice(&instance.to_le_bytes());
                records.extend_from_slice(&[0, 0]);
            }
            let mappings: Vec<(u32, u32)> = self
                .instances
                .iter()
                .enumerate()
                .filter(|(_, list)| !list.is_empty())
                .map(|(i, list)| (list.len() as u32, i as u32))
                .collect();

            let mut header = [0u32; 30];
            header[1] = self.version;
            header[4] = self.structs.len() as u32;
            header[5] = u32::from(prop_count);
            header[7] = mappings.len() as u32;
            header[8] = record_count;
            header[12] = self.int32s.len() as u32;
            header[18] = self.singles.len() as u32;
            header[24] = self.strongs.len() as u32;
            header[28] = self.text1.len() as u32;
            header[29] = self.text2.len() as u32;

            let mut out: Vec<u8> = header.iter().flat_map(|v| v.to_le_bytes()).collect();
            out.extend_from_slice(&structs);
            out.extend_from_slice(&props);
            for (count, index) in &mappings {
                out.extend_from_slice(&count.to_le_bytes());
                out.extend_from_slice(&index.to_le_bytes());
            }
            out.extend_from_slice(&records);
            // Pools in file order; only i32, single and strong are used.
            for v in &self.int32s {
                out.extend_from_slice(&v.to_le_bytes());
            }
            for v in &self.singles {
                out.extend_from_slice(&v.to_le_bytes());
            }
            for (s, i) in &self.strongs {
                out.extend_from_slice(&s.to_le_bytes());
                out.extend_from_slice(&i.to_le_bytes());
            }
            out.extend_from_slice(&self.text1);
            out.extend_from_slice(&self.text2);
            for (_, index) in &mappings {
                for inst in &self.instances[*index as usize] {
                    assert_eq!(inst.len(), sizes[*index as usize], "instance size");
                    out.extend_from_slice(inst);
                }
            }
            out
        }
    }

    fn intern(table: &mut Vec<u8>, s: &str) -> u32 {
        let offset = table.len() as u32;
        table.extend_from_slice(s.as_bytes());
        table.push(0);
        offset
    }

    /// Little-endian helpers for instance bytes.
    pub fn le32(v: u32) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }

    pub fn ptr_bytes(struct_index: u32, instance: u32) -> Vec<u8> {
        [le32(struct_index), le32(instance)].concat()
    }

    /// A database exercising every scalar type, inheritance, inline classes,
    /// arrays and pointers.
    fn sample() -> DataCore {
        sample_version(6)
    }

    fn sample_version(version: u32) -> DataCore {
        let mut b = Builder::new();
        b.version = version;
        let base = b.add_struct(S {
            name: "Base",
            parent: NONE,
            props: vec![prop("id", ty::U32)],
        });
        let inner = b.add_struct(S {
            name: "Inner",
            parent: NONE,
            props: vec![prop("flag", ty::BOOL), prop("small", ty::I8)],
        });
        let all = b.add_struct(S {
            name: "All",
            parent: base,
            props: vec![
                prop("i16", ty::I16),
                prop("i32", ty::I32),
                prop("i64", ty::I64),
                prop("u8", ty::U8),
                prop("u16", ty::U16),
                prop("u64", ty::U64),
                prop("f", ty::SINGLE),
                prop("d", ty::DOUBLE),
                prop("s", ty::STRING),
                prop("loc", ty::LOCALE),
                prop("e", ty::ENUM),
                prop("g", ty::GUID),
                prop("r", ty::REFERENCE),
                P {
                    target: 1,
                    ..prop("inner", ty::CLASS)
                },
                prop("weak", ty::WEAK),
                prop("strong", ty::STRONG),
                array("floats", ty::SINGLE, 0),
                array("ints", ty::I32, 0),
                array("children", ty::CLASS, 1),
            ],
        });
        assert_eq!(inner, 1);
        b.singles = vec![1.5, 2.5];
        b.int32s = vec![-7];
        let s = b.text1("hello");
        let data = [
            le32(42),
            (-2i16).to_le_bytes().to_vec(),
            (-3i32).to_le_bytes().to_vec(),
            (-4i64).to_le_bytes().to_vec(),
            vec![5],
            6u16.to_le_bytes().to_vec(),
            7u64.to_le_bytes().to_vec(),
            0.5f32.to_le_bytes().to_vec(),
            0.25f64.to_le_bytes().to_vec(),
            le32(s),
            le32(s),
            le32(s),
            (0u8..16).collect(),
            [le32(0), (0u8..16).collect()].concat(),
            vec![1, 0xff],
            ptr_bytes(NONE, NONE),
            ptr_bytes(inner, 0),
            [le32(2), le32(0)].concat(),
            [le32(1), le32(0)].concat(),
            [le32(2), le32(0)].concat(),
        ]
        .concat();
        let ptr = Ptr {
            struct_index: all,
            instance: b.add_instance(all, data),
        };
        b.add_instance(inner, vec![1, 3]);
        b.add_instance(inner, vec![0, 4]);
        b.add_record("All.Thing", "libs/thing.xml", ptr);
        DataCore::parse(b.finish()).unwrap()
    }

    #[test]
    fn decodes_every_type() {
        let db = sample();
        let rec = db.records().next().unwrap();
        assert_eq!(rec.name, "All.Thing");
        assert_eq!(rec.file_name, "libs/thing.xml");
        assert_eq!(rec.struct_name, "All");
        assert!(db.is_a(rec.ptr.struct_index, "Base"));
        assert!(!db.is_a(rec.ptr.struct_index, "Inner"));
        assert_eq!(db.struct_count(), 3);
        assert_eq!(db.enum_count(), 0);

        let f = |name| db.field(rec.ptr, name).unwrap().unwrap();
        assert_eq!(f("id"), Value::UInt(42));
        assert_eq!(f("i16"), Value::Int(-2));
        assert_eq!(f("i32"), Value::Int(-3));
        assert_eq!(f("i64"), Value::Int(-4));
        assert_eq!(f("u8"), Value::UInt(5));
        assert_eq!(f("u16"), Value::UInt(6));
        assert_eq!(f("u64"), Value::UInt(7));
        assert_eq!(f("f"), Value::Float(0.5));
        assert_eq!(f("d"), Value::Float(0.25));
        assert_eq!(f("s"), Value::Str("hello".into()));
        assert_eq!(
            f("g"),
            Value::Guid("03020100-0504-0706-0809-0a0b0c0d0e0f".into())
        );
        assert!(matches!(f("r"), Value::Reference(_)));
        assert_eq!(
            f("inner"),
            Value::Class(vec![
                ("flag".into(), Value::Bool(true)),
                ("small".into(), Value::Int(-1)),
            ])
        );
        assert_eq!(f("weak"), Value::Null);
        assert_eq!(
            f("floats"),
            Value::Array(vec![Value::Float(1.5), Value::Float(2.5)])
        );
        assert_eq!(f("ints"), Value::Array(vec![Value::Int(-7)]));
        assert_eq!(db.field(rec.ptr, "missing").unwrap(), None);

        let json = db.to_json(rec.ptr, 1).unwrap();
        assert_eq!(json["_type"], "All");
        assert_eq!(json["strong"]["small"], 3);
        assert_eq!(json["children"][1]["small"], 4);
        assert_eq!(json["r"].as_str().unwrap().len(), 40);
        let shallow = db.to_json(rec.ptr, 0).unwrap();
        assert_eq!(shallow["strong"], "Inner[0]");
    }

    #[test]
    fn reads_version_8_records() {
        let db = sample_version(8);
        let rec = db.records().next().unwrap();
        assert_eq!(rec.name, "All.Thing");
        assert_eq!(rec.file_name, "libs/thing.xml");
        assert_eq!(db.field(rec.ptr, "id").unwrap(), Some(Value::UInt(42)));
    }

    #[test]
    fn rejects_bad_pointers() {
        let db = sample();
        let missing = Ptr {
            struct_index: 9,
            instance: 0,
        };
        assert!(matches!(
            db.fields(missing),
            Err(DataCoreError::BadIndex(_))
        ));
        let past = Ptr {
            struct_index: 1,
            instance: 5,
        };
        assert!(matches!(db.fields(past), Err(DataCoreError::BadIndex(_))));
        assert_eq!(db.struct_name(99), "?");
    }

    #[test]
    fn rejects_old_versions_and_bad_layouts() {
        let mut b = Builder::new();
        b.version = 5;
        assert!(matches!(
            DataCore::parse(b.finish()),
            Err(DataCoreError::Layout(_))
        ));

        let mut b = Builder::new();
        let s = b.add_struct(S {
            name: "X",
            parent: NONE,
            props: vec![prop("v", ty::U32)],
        });
        b.add_instance(s, le32(1));
        let mut bytes = b.finish();
        bytes.push(0);
        assert!(matches!(
            DataCore::parse(bytes.clone()),
            Err(DataCoreError::Layout(_))
        ));
        assert!(matches!(
            DataCore::parse(bytes[..100].to_vec()),
            Err(DataCoreError::Truncated(_))
        ));

        // Declared size 4 but the property is a u64: fields don't add up.
        let mut b = Builder::new();
        b.add_struct(S {
            name: "Y",
            parent: NONE,
            props: vec![prop("v", ty::U64)],
        });
        let mut bytes = b.finish();
        let size_at = HEADER_LEN + 12;
        bytes[size_at..size_at + 4].copy_from_slice(&4u32.to_le_bytes());
        let err = DataCore::parse(bytes).err().unwrap();
        assert!(err.to_string().contains("struct Y"), "{err}");
    }
}
