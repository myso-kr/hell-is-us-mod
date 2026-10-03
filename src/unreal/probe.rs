//! `hiumod doctor <inspect|find|dump|watch|scan>`: look inside the running game by
//! its own reflection, from outside, writing nothing (.spec/DOCTOR.md).
//!
//! Everything an in-process tool such as UE4SS shows by name — classes, properties,
//! their types and offsets, live objects — comes from the same reflection data this
//! crate already walks. What reflection does not name (native fields: an item stack's
//! count, say) is shown as the gaps between named fields, and found by watching it
//! change or by scanning an object for a value and narrowing as it changes.

use crate::mem::{self, Memory};
use crate::names::{Names, Property, CLASS, OUTER};
use std::collections::HashSet;
use std::fmt::Write as _;

/// Below UObject, every object is these: their fields are not worth printing.
const BASE: [&str; 1] = ["Object"];
/// An array's elements shown by `inspect`.
const SHOWN: usize = 8;

/// One property's name, type and place, and the struct or class it belongs to.
struct Field {
    owner: String,
    p: Property,
    ty: String,
}

/// Every reflected field of an object's class and its supers (nearest last, so the
/// output reads in memory order), without UObject's own.
fn fields_of(n: &Names, m: &dyn Memory, strukt: u64) -> Vec<Field> {
    let mut out = Vec::new();
    for c in n.lineage(m, strukt).into_iter().rev() {
        let owner = n.object(m, c).unwrap_or_default();
        if BASE.contains(&owner.as_str()) {
            continue;
        }
        for p in n.properties(m, c) {
            let ty = n.field_type(m, p.field).unwrap_or_default();
            out.push(Field { owner: owner.clone(), p, ty });
        }
    }
    out.sort_by_key(|f| f.p.offset);
    out
}

fn class_of(m: &dyn Memory, obj: u64) -> Option<u64> {
    mem::read_u64(m, obj + CLASS).filter(|&c| mem::plausible(c))
}

fn label(n: &Names, m: &dyn Memory, obj: u64) -> String {
    let name = n.object(m, obj).unwrap_or_else(|| "?".into());
    let class = class_of(m, obj).and_then(|c| n.object(m, c)).unwrap_or_else(|| "?".into());
    format!("{name} [{class}] @{obj:#x}")
}

/// An FString's text: TArray<wchar>.
fn fstring(m: &dyn Memory, at: u64) -> Option<String> {
    let (data, num) = (mem::read_u64(m, at)?, mem::read_u32(m, at + 8)?);
    if num == 0 {
        return Some(String::new());
    }
    if !mem::plausible(data) || num > 4096 {
        return None;
    }
    let mut b = vec![0u8; num as usize * 2];
    m.read(data, &mut b).then_some(())?;
    let w: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
    Some(String::from_utf16_lossy(&w))
}

/// A scalar property's value as text, or None for the kinds `inspect` expands itself.
fn scalar(n: &Names, m: &dyn Memory, at: u64, f: &Field) -> Option<String> {
    let s = match (f.ty.as_str(), f.p.size) {
        ("FloatProperty", _) => format!("{}", mem::read_f32(m, at)?),
        ("DoubleProperty", _) => format!("{}", f64::from_bits(mem::read_u64(m, at)?)),
        ("IntProperty", _) => format!("{}", mem::read_u32(m, at)? as i32),
        ("UInt32Property", _) => format!("{}", mem::read_u32(m, at)?),
        ("Int64Property", _) => format!("{}", mem::read_u64(m, at)? as i64),
        ("UInt64Property", _) => format!("{}", mem::read_u64(m, at)?),
        ("Int16Property" | "UInt16Property", _) => format!("{}", mem::read_u32(m, at)? & 0xFFFF),
        ("ByteProperty" | "Int8Property" | "EnumProperty", 1) => format!("{}", mem::read_u32(m, at)? & 0xFF),
        ("EnumProperty", 4) => format!("{}", mem::read_u32(m, at)?),
        // The bit a bool uses is in the property; the whole byte is shown.
        ("BoolProperty", _) => format!("byte {:#04x}", mem::read_u32(m, at)? & 0xFF),
        ("NameProperty", _) => n.get(m, mem::read_u32(m, at)?).unwrap_or_else(|| "?".into()),
        ("StrProperty", _) => format!("{:?}", fstring(m, at)?),
        ("TextProperty", _) => "(FText)".into(),
        ("SoftObjectProperty" | "SoftClassProperty", _) => {
            // TSoftObjectPtr: a weak pointer, then FSoftObjectPath — package FName at +0x8,
            // asset FName at +0x10 (as KnownFacts reads them, knowledge.rs).
            let package = n.get(m, mem::read_u32(m, at + 0x8)?).unwrap_or_default();
            let asset = n.get(m, mem::read_u32(m, at + 0x10)?).unwrap_or_default();
            format!("soft {package}.{asset}")
        }
        ("WeakObjectProperty", _) => format!("weak #{}", mem::read_u32(m, at)?),
        ("MapProperty" | "SetProperty", _) => format!("{} entries", mem::read_u32(m, at + 8)?),
        ("DelegateProperty" | "MulticastInlineDelegateProperty" | "MulticastSparseDelegateProperty", _) => {
            "(delegate)".into()
        }
        _ => return None,
    };
    Some(s)
}

/// UStruct::PropertiesSize: how big instances of a class or struct are (bytes).
/// Not reflected; on UE 5.5 it follows ChildProperties (0x50) at 0x58. Checked
/// against the reflected fields — it can only be at least where the last one ends.
pub fn size_of(n: &Names, m: &dyn Memory, strukt: u64) -> Option<u32> {
    let size = mem::read_u32(m, strukt + 0x58)?;
    let end = fields_of(n, m, strukt).iter().map(|f| f.p.offset + f.p.size).max().unwrap_or(0);
    (size >= end && size < 0x10_0000).then_some(size)
}

/// What `inspect` prints, and how deep it follows.
pub struct Inspect<'a> {
    pub n: &'a Names,
    pub m: &'a dyn Memory,
    /// Object pointers followed this many levels.
    pub depth: usize,
    /// Native gaps between reflected fields shown as raw words.
    pub gaps: bool,
    seen: HashSet<u64>,
    pub out: String,
}

impl<'a> Inspect<'a> {
    pub fn new(n: &'a Names, m: &'a dyn Memory, depth: usize, gaps: bool) -> Inspect<'a> {
        Inspect { n, m, depth, gaps, seen: HashSet::new(), out: String::new() }
    }

    fn line(&mut self, indent: usize, text: impl AsRef<str>) {
        let _ = writeln!(self.out, "{}{}", "  ".repeat(indent), text.as_ref());
    }

    /// An object: its class lineage, then every field.
    pub fn object(&mut self, obj: u64, indent: usize, depth: usize) {
        let (n, m) = (self.n, self.m);
        let Some(class) = class_of(m, obj) else {
            self.line(indent, format!("{obj:#x}: not an object"));
            return;
        };
        if !self.seen.insert(obj) {
            self.line(indent, format!("{} (shown above)", label(n, m, obj)));
            return;
        }
        let lineage: Vec<String> = n.lineage(m, class).into_iter().filter_map(|c| n.object(m, c)).collect();
        self.line(indent, format!("{}  < {}", label(n, m, obj), lineage.join(" < ")));
        if let Some(outer) = mem::read_u64(m, obj + OUTER).filter(|&o| mem::plausible(o)) {
            self.line(indent + 1, format!("outer: {}", label(n, m, outer)));
        }
        self.fields(obj, class, indent + 1, depth);
    }

    /// A struct's or object's fields at `base`.
    fn fields(&mut self, base: u64, strukt: u64, indent: usize, depth: usize) {
        let (n, m) = (self.n, self.m);
        let fields = fields_of(n, m, strukt);
        // UObject's own header ends at 0x28.
        let object = mem::read_u64(m, base + CLASS) == Some(strukt);
        let mut cursor = if object { 0x28u32 } else { 0 };
        let mut owner = String::new();
        for f in fields {
            if self.gaps && f.p.offset > cursor + 3 {
                self.gap(base, cursor, f.p.offset, indent);
            }
            cursor = cursor.max(f.p.offset + f.p.size);
            if f.owner != owner {
                owner = f.owner.clone();
                self.line(indent, format!("— {owner}"));
            }
            self.field(base, &f, indent, depth);
        }
        // Past the last named field, to the end of the instance: native fields too.
        if self.gaps {
            if let Some(end) = size_of(n, m, strukt).filter(|&e| e > cursor + 3) {
                self.gap(base, cursor, end, indent);
            }
        }
    }

    /// Bytes no reflected field names: as u32 words, with a float reading where one
    /// looks like a float — native fields hide here.
    fn gap(&mut self, base: u64, from: u32, to: u32, indent: usize) {
        let len = (to - from).min(0x100) as usize;
        let mut b = vec![0u8; len];
        if !self.m.read(base + from as u64, &mut b) {
            return;
        }
        let words: Vec<String> = b
            .chunks_exact(4)
            .map(|c| {
                let v = u32::from_le_bytes(c.try_into().unwrap());
                let f = f32::from_bits(v);
                if v == 0 {
                    "0".into()
                } else if f.is_normal() && f.abs() > 1e-4 && f.abs() < 1e7 && v > 0x0010_0000 {
                    format!("{v:#x}({f:.3})")
                } else {
                    format!("{v:#x}")
                }
            })
            .collect();
        self.line(indent, format!("· native +{from:#x}..+{to:#x}: {}", words.join(" ")));
    }

    fn field(&mut self, base: u64, f: &Field, indent: usize, depth: usize) {
        let (n, m) = (self.n, self.m);
        let at = base + f.p.offset as u64;
        let head = format!("+{:#05x} {} {}", f.p.offset, f.ty.trim_end_matches("Property"), f.p.name);
        if let Some(v) = scalar(n, m, at, f) {
            self.line(indent, format!("{head} = {v}"));
            return;
        }
        match f.ty.as_str() {
            "ObjectProperty" | "ClassProperty" | "InterfaceProperty" => {
                match mem::read_u64(m, at).filter(|&p| mem::plausible(p)) {
                    None => self.line(indent, format!("{head} = null")),
                    Some(p) if depth > 0 && f.ty == "ObjectProperty" => {
                        self.line(indent, format!("{head} →"));
                        self.object(p, indent + 1, depth - 1);
                    }
                    Some(p) => self.line(indent, format!("{head} = {}", label(n, m, p))),
                }
            }
            "StructProperty" => match n.struct_of(m, f.p.field) {
                Some(st) => {
                    let sn = n.object(m, st).unwrap_or_default();
                    self.line(indent, format!("{head} : {sn}"));
                    self.fields(at, st, indent + 1, depth);
                }
                None => self.line(indent, format!("{head} (struct ?)")),
            },
            "ArrayProperty" => self.array(at, f, &head, indent, depth),
            _ => {
                let mut b = vec![0u8; (f.p.size as usize).min(16)];
                m.read(at, &mut b);
                self.line(indent, format!("{head} = {b:02x?}"));
            }
        }
    }

    fn array(&mut self, at: u64, f: &Field, head: &str, indent: usize, depth: usize) {
        let (n, m) = (self.n, self.m);
        let (data, num) = (mem::read_u64(m, at).unwrap_or(0), mem::read_u32(m, at + 8).unwrap_or(0));
        self.line(indent, format!("{head} [{num}]"));
        let Some(inner) = n.inner_of(m, f.p.field) else { return };
        if num == 0 || !mem::plausible(data) {
            return;
        }
        let size = mem::read_u32(m, inner + n.layout.size).unwrap_or(0) as u64;
        let ty = n.field_type(m, inner).unwrap_or_default();
        let el = Field {
            owner: String::new(),
            p: Property { name: String::new(), offset: 0, size: size as u32, field: inner },
            ty: ty.clone(),
        };
        for i in 0..(num as usize).min(SHOWN) {
            let e = data + i as u64 * size;
            if let Some(v) = scalar(n, m, e, &el) {
                self.line(indent + 1, format!("[{i}] {v}"));
                continue;
            }
            match ty.as_str() {
                "ObjectProperty" => match mem::read_u64(m, e).filter(|&p| mem::plausible(p)) {
                    Some(p) if depth > 0 => {
                        self.line(indent + 1, format!("[{i}] →"));
                        self.object(p, indent + 2, depth - 1);
                    }
                    Some(p) => self.line(indent + 1, format!("[{i}] {}", label(n, m, p))),
                    None => self.line(indent + 1, format!("[{i}] null")),
                },
                "StructProperty" => {
                    self.line(indent + 1, format!("[{i}]"));
                    if let Some(st) = n.struct_of(m, inner) {
                        self.fields(e, st, indent + 2, depth);
                    }
                }
                _ => self.line(indent + 1, format!("[{i}] ({ty})")),
            }
        }
        if num as usize > SHOWN {
            self.line(indent + 1, format!("… {} more", num as usize - SHOWN));
        }
    }
}

/// Every class and struct among `structs` with a property whose name holds `text`
/// (any case): `Owner.Property type @+offset size`.
pub fn find(n: &Names, m: &dyn Memory, structs: &[u64], text: &str) -> Vec<String> {
    let text = text.to_lowercase();
    let mut out = Vec::new();
    for &s in structs {
        let owner = n.object(m, s).unwrap_or_default();
        for p in n.properties(m, s) {
            if p.name.to_lowercase().contains(&text) {
                let ty = n.field_type(m, p.field).unwrap_or_default();
                out.push(format!("{owner}.{} {} @+{:#x} ({} B)", p.name, ty, p.offset, p.size));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The classes and structs (as objects) among `objects`, optionally only those whose
/// names start with one of `prefixes`.
pub fn structs(n: &Names, m: &dyn Memory, objects: &[u64], prefixes: &[String]) -> Vec<u64> {
    const KINDS: [&str; 4] = ["Class", "BlueprintGeneratedClass", "ScriptStruct", "WidgetBlueprintGeneratedClass"];
    objects
        .iter()
        .copied()
        .filter(|&o| class_of(m, o).and_then(|c| n.object(m, c)).is_some_and(|k| KINDS.contains(&k.as_str())))
        .filter(|&o| {
            prefixes.is_empty() || n.object(m, o).is_some_and(|name| prefixes.iter().any(|p| name.starts_with(p)))
        })
        .collect()
}

/// An SDK-like listing: each class or struct, its super, and its own properties.
pub fn dump(n: &Names, m: &dyn Memory, structs: &[u64]) -> String {
    let mut rows: Vec<(String, String)> = structs
        .iter()
        .map(|&s| {
            let name = n.object(m, s).unwrap_or_default();
            let kind = class_of(m, s).and_then(|c| n.object(m, c)).unwrap_or_default();
            let sup = n.lineage(m, s).get(1).and_then(|&c| n.object(m, c)).unwrap_or_default();
            let mut text =
                format!("{kind} {name}{}\n", if sup.is_empty() { String::new() } else { format!(" : {sup}") });
            let mut props = n.properties(m, s);
            props.sort_by_key(|p| p.offset);
            for p in props {
                let ty = n.field_type(m, p.field).unwrap_or_default();
                let detail = match ty.as_str() {
                    "StructProperty" => n.struct_of(m, p.field).and_then(|st| n.object(m, st)).unwrap_or_default(),
                    "ArrayProperty" | "SetProperty" => n
                        .inner_of(m, p.field)
                        .map(|i| {
                            let it = n.field_type(m, i).unwrap_or_default();
                            match n.struct_of(m, i).and_then(|st| n.object(m, st)) {
                                Some(sn) => format!("{it}<{sn}>"),
                                None => it,
                            }
                        })
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                let _ = writeln!(text, "    +{:#06x} {:>5} B  {ty} {} {detail}", p.offset, p.size, p.name);
            }
            (name, text)
        })
        .collect();
    rows.sort();
    rows.dedup_by(|a, b| a.0 == b.0);
    rows.into_iter().map(|(_, t)| t).collect::<Vec<_>>().join("\n")
}

/// How far an object's reflected fields reach (bytes): where `watch` and `scan` stop
/// by default, with room past the last one for native fields.
pub fn extent(n: &Names, m: &dyn Memory, obj: u64) -> u64 {
    let Some(class) = class_of(m, obj) else { return 0x100 };
    if let Some(size) = size_of(n, m, class) {
        return (size as u64).clamp(0x30, 0x4000);
    }
    let end = fields_of(n, m, class).iter().map(|f| (f.p.offset + f.p.size) as u64).max().unwrap_or(0x28);
    (end + 0x80).min(0x4000)
}

/// What changed between two reads of an object: each reflected field that differs,
/// by name, and every other u32 that differs, by offset.
pub fn diff(n: &Names, m: &dyn Memory, obj: u64, before: &[u8], after: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let Some(class) = class_of(m, obj) else { return out };
    let fields = fields_of(n, m, class);
    let mut named = vec![false; before.len()];
    for f in &fields {
        let (a, b) = (f.p.offset as usize, (f.p.offset + f.p.size) as usize);
        if b > before.len() {
            continue;
        }
        named[a..b].iter_mut().for_each(|x| *x = true);
        if before[a..b] != after[a..b] {
            let show = |bytes: &[u8]| match (f.ty.as_str(), bytes.len()) {
                ("FloatProperty", 4) => format!("{}", f32::from_le_bytes(bytes.try_into().unwrap())),
                ("DoubleProperty", 8) => format!("{}", f64::from_le_bytes(bytes.try_into().unwrap())),
                ("IntProperty", 4) => format!("{}", i32::from_le_bytes(bytes.try_into().unwrap())),
                (_, n) if n <= 8 => format!("{:02x?}", bytes),
                _ => "(changed)".into(),
            };
            out.push(format!("+{:#x} {} : {} → {}", f.p.offset, f.p.name, show(&before[a..b]), show(&after[a..b])));
        }
    }
    for i in (0..before.len().saturating_sub(3)).step_by(4) {
        if !named[i] && before[i..i + 4] != after[i..i + 4] {
            let (x, y) = (
                u32::from_le_bytes(before[i..i + 4].try_into().unwrap()),
                u32::from_le_bytes(after[i..i + 4].try_into().unwrap()),
            );
            out.push(format!("+{i:#x} (native) : {x} → {y}   (f32 {} → {})", f32::from_bits(x), f32::from_bits(y)));
        }
    }
    out
}

/// Where in `bytes` a value sits, as each width it could be stored in: (offset, kind).
pub fn matches(bytes: &[u8], v: f64) -> Vec<(usize, &'static str)> {
    let mut out = Vec::new();
    for i in (0..bytes.len().saturating_sub(3)).step_by(4) {
        let w = u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        if v.fract() == 0.0 && v >= i32::MIN as f64 && v <= u32::MAX as f64 && (w as f64 == v || w as i32 as f64 == v) {
            out.push((i, "u32"));
        }
        if (f32::from_bits(w) as f64 - v).abs() <= 1e-3 * v.abs().max(1.0) && w != 0 {
            out.push((i, "f32"));
        }
        if i % 8 == 0 && i + 8 <= bytes.len() {
            let d = f64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
            if (d - v).abs() <= 1e-6 * v.abs().max(1.0) && d != 0.0 {
                out.push((i, "f64"));
            }
        }
    }
    out
}

/// A candidate's current value, in its kind.
pub fn read_as(m: &dyn Memory, at: u64, kind: &str) -> Option<f64> {
    match kind {
        "u32" => mem::read_u32(m, at).map(|v| v as f64),
        "f32" => mem::read_f32(m, at).map(|v| v as f64),
        "f64" => mem::read_u64(m, at).map(f64::from_bits),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_found_in_every_width() {
        let mut b = vec![0u8; 32];
        b[4..8].copy_from_slice(&7u32.to_le_bytes());
        b[8..12].copy_from_slice(&7.0f32.to_le_bytes());
        b[16..24].copy_from_slice(&7.0f64.to_le_bytes());
        let m = matches(&b, 7.0);
        assert!(m.contains(&(4, "u32")));
        assert!(m.contains(&(8, "f32")));
        assert!(m.contains(&(16, "f64")));
        assert!(!m.iter().any(|&(i, _)| i == 0), "zero is not seven");
    }
}
