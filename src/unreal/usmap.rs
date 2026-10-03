//! A `.usmap` mappings file from the game's live reflection: what the survey tool
//! (tools/survey, CUE4Parse) needs to read cooked assets, whose properties are stored
//! unversioned — in schema order, without names (.spec/ITEMS.md §3.1).
//!
//! Written as usmap version 0 (Initial), uncompressed — the plainest form readers take:
//! - u16 magic 0x30C4, u8 version 0, u8 compression 0, u32 size, u32 size
//! - names: u32 count, each u8 length + bytes
//! - enums: u32 count, each u32 name, u8 count, u32 entry names
//! - structs: u32 count, each u32 name, u32 super (or u32::MAX), u16 property count
//!   (array dimensions included), u16 serialised count, then each property: u16 schema
//!   index, u8 array dimension, u32 name, type data
//! - type data: u8 kind, then for an enum its underlying type and u32 enum name, for a
//!   struct u32 struct name, for an array, set or optional its inner type, for a map
//!   key and value types
//!
//! Every class and script struct in GUObjectArray is written (functions are not), with
//! its own properties; supers by name. Enums from `UEnum.Names` (+0x40, (FName, i64)
//! pairs), their `Enum::` prefixes dropped.

use crate::gobjects::Objects;
use crate::mem::{self, Memory};
use crate::names::{Names, CLASS, NAME, SUPER};
use std::collections::HashMap;

const MAGIC: u16 = 0x30C4;
/// FProperty::ArrayDim.
const ARRAY_DIM: u64 = 0x30;
/// UStruct::ChildProperties.
const CHILD_PROPERTIES: u64 = 0x50;
/// UEnum::Names.
const ENUM_NAMES: u64 = 0x40;

/// EPropertyType as usmap numbers it.
fn kind(t: &str) -> Option<u8> {
    Some(match t {
        "ByteProperty" => 0,
        "BoolProperty" => 1,
        "IntProperty" => 2,
        "FloatProperty" => 3,
        "ObjectProperty" | "ClassProperty" | "ObjectPtrProperty" | "ClassPtrProperty" => 4,
        "NameProperty" => 5,
        "DelegateProperty" => 6,
        "DoubleProperty" | "LargeWorldCoordinatesRealProperty" => 7,
        "ArrayProperty" => 8,
        "StructProperty" => 9,
        "StrProperty" => 10,
        "TextProperty" => 11,
        "InterfaceProperty" => 12,
        "MulticastDelegateProperty" | "MulticastInlineDelegateProperty" | "MulticastSparseDelegateProperty" => 13,
        "WeakObjectProperty" => 14,
        "LazyObjectProperty" => 15,
        "SoftObjectProperty" | "SoftClassProperty" => 17,
        "UInt64Property" => 18,
        "UInt32Property" => 19,
        "UInt16Property" => 20,
        "Int64Property" => 21,
        "Int16Property" => 22,
        "Int8Property" => 23,
        "MapProperty" => 24,
        "SetProperty" => 25,
        "EnumProperty" => 26,
        "FieldPathProperty" => 27,
        "OptionalProperty" => 28,
        "Utf8StrProperty" => 29,
        "AnsiStrProperty" => 30,
        _ => return None,
    })
}

struct Writer {
    names: Vec<String>,
    index: HashMap<String, u32>,
}

impl Writer {
    fn name(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let i = self.names.len() as u32;
        self.names.push(s.to_string());
        self.index.insert(s.to_string(), i);
        i
    }
}

/// The pointers past the FProperty base (+0x70) that are properties, in order: an
/// array's or set's inner, a map's key then value, an enum's underlying.
fn sub_properties(m: &dyn Memory, n: &Names, field: u64) -> Vec<u64> {
    (0x70..0x90)
        .step_by(8)
        .filter_map(|o| mem::read_u64(m, field + o).filter(|&p| mem::plausible(p)))
        .filter(|&p| n.field_type(m, p).is_some_and(|t| t.ends_with("Property")))
        .collect()
}

/// The UObject past the FProperty base whose class is `want` (an enum, a struct).
fn object_of(m: &dyn Memory, n: &Names, field: u64, want: &[&str]) -> Option<u64> {
    (0x70..0xA0).step_by(8).find_map(|o| {
        let p = mem::read_u64(m, field + o).filter(|&p| mem::plausible(p))?;
        let c = n.class(m, p)?;
        want.contains(&c.as_str()).then_some(p)
    })
}

/// A property's type data; `None` when it is of a kind usmap has no number for.
fn type_data(m: &dyn Memory, n: &Names, w: &mut Writer, field: u64, depth: u32) -> Option<Vec<u8>> {
    if depth > 4 {
        return None;
    }
    let t = n.field_type(m, field)?;
    let mut out = Vec::new();
    match t.as_str() {
        "ByteProperty" => {
            // A byte that holds an enum is written as that enum over a byte.
            match object_of(m, n, field, &["Enum", "UserDefinedEnum"]) {
                Some(e) => {
                    out.push(26);
                    out.push(0);
                    out.extend(w.name(&n.object(m, e)?).to_le_bytes());
                }
                None => out.push(0),
            }
        }
        "EnumProperty" => {
            out.push(26);
            let under = sub_properties(m, n, field).into_iter().next()?;
            out.extend(type_data(m, n, w, under, depth + 1)?);
            let e = object_of(m, n, field, &["Enum", "UserDefinedEnum"])?;
            out.extend(w.name(&n.object(m, e)?).to_le_bytes());
        }
        "StructProperty" => {
            out.push(9);
            let s = object_of(m, n, field, &["ScriptStruct", "UserDefinedStruct"])?;
            out.extend(w.name(&n.object(m, s)?).to_le_bytes());
        }
        "ArrayProperty" | "SetProperty" | "OptionalProperty" => {
            out.push(kind(&t)?);
            let inner = sub_properties(m, n, field).into_iter().next()?;
            out.extend(type_data(m, n, w, inner, depth + 1)?);
        }
        "MapProperty" => {
            out.push(24);
            let subs = sub_properties(m, n, field);
            let (k, v) = (*subs.first()?, *subs.get(1)?);
            out.extend(type_data(m, n, w, k, depth + 1)?);
            out.extend(type_data(m, n, w, v, depth + 1)?);
        }
        other => out.push(kind(other)?),
    }
    Some(out)
}

/// Build the mappings from every struct, class and enum the game has loaded. Returns
/// the file's bytes and (structs, enums) written.
pub fn build(m: &dyn Memory, n: &Names, objects: &Objects) -> (Vec<u8>, usize, usize) {
    let l = n.layout;
    let mut w = Writer { names: Vec::new(), index: HashMap::new() };
    let mut enums: Vec<u8> = Vec::new();
    let mut structs: Vec<u8> = Vec::new();
    let (mut ne, mut ns) = (0u32, 0u32);
    let mut kinds: HashMap<u64, u8> = HashMap::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for o in objects.all(m) {
        let Some(class) = mem::read_u64(m, o + CLASS).filter(|&c| mem::plausible(c)) else { continue };
        // 0: other, 1: struct or class, 2: enum.
        let k = *kinds.entry(class).or_insert_with(|| {
            let lineage: Vec<String> = n.lineage(m, class).into_iter().filter_map(|c| n.object(m, c)).collect();
            if lineage.iter().any(|c| c == "Function") {
                0
            } else if lineage.iter().any(|c| c == "Struct") {
                1
            } else if lineage.iter().any(|c| c == "Enum") {
                2
            } else {
                0
            }
        });
        if k == 0 {
            continue;
        }
        let Some(name) = mem::read_u32(m, o + NAME).and_then(|i| n.get(m, i)) else { continue };
        if name.starts_with("Default__") || !seen_names.insert(format!("{k}{name}")) {
            continue;
        }
        if k == 2 {
            let (Some(data), Some(count)) = (mem::read_u64(m, o + ENUM_NAMES), mem::read_u32(m, o + ENUM_NAMES + 8))
            else {
                continue;
            };
            if !mem::plausible(data) || count > 4096 {
                continue;
            }
            let entries: Vec<String> = (0..count.min(255) as u64)
                .filter_map(|i| mem::read_u32(m, data + i * 16).and_then(|x| n.get(m, x)))
                .map(|s| s.rsplit("::").next().unwrap_or(&s).to_string())
                .collect();
            enums.extend(w.name(&name).to_le_bytes());
            enums.push(entries.len() as u8);
            for e in &entries {
                enums.extend(w.name(e).to_le_bytes());
            }
            ne += 1;
            continue;
        }
        // A struct or class: its own properties.
        let sup = mem::read_u64(m, o + SUPER)
            .filter(|&p| mem::plausible(p))
            .and_then(|p| mem::read_u32(m, p + NAME))
            .and_then(|i| n.get(m, i));
        let mut props: Vec<u8> = Vec::new();
        let (mut index, mut serial) = (0u32, 0u32);
        let mut field = mem::read_u64(m, o + CHILD_PROPERTIES).unwrap_or(0);
        let mut guard = 0;
        while mem::plausible(field) && guard < 2048 {
            guard += 1;
            let next = mem::read_u64(m, field + l.next).unwrap_or(0);
            let dim = mem::read_u32(m, field + ARRAY_DIM).unwrap_or(1).clamp(1, 255);
            let pname = mem::read_u32(m, field + l.name).and_then(|i| n.get(m, i));
            if let (Some(pname), Some(td)) = (pname, type_data(m, n, &mut w, field, 0)) {
                props.extend((index as u16).to_le_bytes());
                props.push(dim as u8);
                props.extend(w.name(&pname).to_le_bytes());
                props.extend(td);
                serial += 1;
            }
            // A property of an unknown kind still takes its place in the schema.
            index += dim;
            field = next;
        }
        structs.extend(w.name(&name).to_le_bytes());
        structs.extend(sup.map_or(u32::MAX, |s| w.name(&s)).to_le_bytes());
        structs.extend((index as u16).to_le_bytes());
        structs.extend((serial as u16).to_le_bytes());
        structs.extend(props);
        ns += 1;
    }
    let mut body = Vec::new();
    body.extend((w.names.len() as u32).to_le_bytes());
    for s in &w.names {
        let b = s.as_bytes();
        let b = &b[..b.len().min(255)];
        body.push(b.len() as u8);
        body.extend(b);
    }
    body.extend(ne.to_le_bytes());
    body.extend(enums);
    body.extend(ns.to_le_bytes());
    body.extend(structs);
    let mut out = Vec::with_capacity(body.len() + 12);
    out.extend(MAGIC.to_le_bytes());
    out.push(0);
    out.push(0);
    out.extend((body.len() as u32).to_le_bytes());
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    (out, ns as usize, ne as usize)
}
