//! The engine's name table, and the class name of any object through it.
//!
//! Every UObject carries `ClassPrivate` (+0x10), and every UClass its own name as an
//! `FName` (+0x18): a 32-bit index into `FNamePool`. The pool hands out names in
//! 128 KiB blocks; an index is (block << 16 | offset / 2), and an entry is a 16-bit
//! header — wide flag in bit 0, length in the top ten bits — then the characters.
//!
//! Where the pool is, is build-specific and found once by `discover`: the game image's
//! writable data holds `FNamePool`, whose block list starts with block 0 — and block 0
//! of every UE5 build starts with the same two names, `None` then `ByteProperty`.

use crate::mem::{self, Memory};

/// UObjectBase::ClassPrivate and ::NamePrivate.
pub const CLASS: u64 = 0x10;
pub const NAME: u64 = 0x18;
/// UObjectBase::OuterPrivate.
pub const OUTER: u64 = 0x20;

/// UStruct::SuperStruct and ::ChildProperties. Engine layout, not game layout: the
/// same from UE 5.0 through 5.6 in non-editor builds.
pub const SUPER: u64 = 0x40;
const CHILD_PROPERTIES: u64 = 0x50;
/// A class with more properties than this is not being read right.
const MAX_PROPERTIES: usize = 512;
/// Deeper inheritance than this is a loop, not a class.
const MAX_DEPTH: usize = 32;

/// Where FField and FProperty keep the fields this tool reads. These moved between
/// engine versions (FFieldVariant shrank from 16 bytes to 8), so the candidates are
/// tried against the live game and the one that reads a known class correctly wins
/// (anchors.rs). The first is what Minecraft Dungeons II's UE 5.6 measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub next: u64,
    pub name: u64,
    pub size: u64,
    pub offset: u64,
}

pub const LAYOUTS: [Layout; 4] = [
    Layout { next: 0x18, name: 0x20, size: 0x34, offset: 0x48 },
    Layout { next: 0x18, name: 0x20, size: 0x34, offset: 0x44 },
    Layout { next: 0x20, name: 0x28, size: 0x3C, offset: 0x4C },
    Layout { next: 0x20, name: 0x28, size: 0x3C, offset: 0x50 },
];

/// One property a class declares.
#[derive(Debug, PartialEq)]
pub struct Property {
    pub name: String,
    pub offset: u32,
    pub size: u32,
    /// The FField itself — where a struct property says which struct it holds.
    pub field: u64,
}

/// FNameEntryAllocator: an 8-byte lock, CurrentBlock, CurrentByteCursor, Blocks[].
const BLOCKS: u64 = 0x10;
const MAX_BLOCKS: u32 = 8192;
const BLOCK_BYTES: u32 = 0x20000;

pub struct Names {
    pub pool: u64,
    pub layout: Layout,
}

impl Names {
    pub fn new(pool: u64) -> Names {
        Names { pool, layout: LAYOUTS[0] }
    }

    pub fn with_layout(pool: u64, layout: Layout) -> Names {
        Names { pool, layout }
    }

    /// The string an FName index stands for. `None` for anything that does not
    /// decode to a plausible name — never a guess.
    pub fn get(&self, m: &dyn Memory, index: u32) -> Option<String> {
        let (block, offset) = (index >> 16, (index & 0xFFFF) as u64 * 2);
        if block >= MAX_BLOCKS {
            return None;
        }
        let base = mem::read_u64(m, self.pool + BLOCKS + block as u64 * 8).filter(|&p| mem::plausible(p))?;
        let mut h = [0u8; 2];
        m.read(base + offset, &mut h).then_some(())?;
        let header = u16::from_le_bytes(h);
        let (wide, len) = (header & 1 == 1, (header >> 6) as usize);
        if len == 0 || len > 1024 {
            return None;
        }
        if wide {
            let mut b = vec![0u8; len * 2];
            m.read(base + offset + 2, &mut b).then_some(())?;
            let units: Vec<u16> = b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            String::from_utf16(&units).ok()
        } else {
            let mut b = vec![0u8; len];
            m.read(base + offset + 2, &mut b).then_some(())?;
            b.iter().all(|c| c.is_ascii_graphic() || *c == b' ').then(|| b.iter().map(|&c| c as char).collect())
        }
    }

    /// The name an object carries — its own, not its class's.
    pub fn object(&self, m: &dyn Memory, obj: u64) -> Option<String> {
        self.get(m, mem::read_u32(m, obj + NAME)?)
    }

    /// The name of an object's class: `ATR_Loot`, `CharacterMovementComponent`.
    pub fn class(&self, m: &dyn Memory, obj: u64) -> Option<String> {
        let class = mem::read_u64(m, obj + CLASS).filter(|&p| mem::plausible(p))?;
        self.object(m, class)
    }

    /// The properties a class declares itself — not those it inherits. A field
    /// whose name does not decode ends the walk: what follows it cannot be trusted.
    pub fn properties(&self, m: &dyn Memory, class: u64) -> Vec<Property> {
        let l = self.layout;
        let mut out = Vec::new();
        let mut field = mem::read_u64(m, class + CHILD_PROPERTIES).unwrap_or(0);
        while mem::plausible(field) && out.len() < MAX_PROPERTIES {
            let Some(name) = mem::read_u32(m, field + l.name).and_then(|i| self.get(m, i)) else { break };
            let (Some(size), Some(offset)) = (mem::read_u32(m, field + l.size), mem::read_u32(m, field + l.offset))
            else {
                break;
            };
            out.push(Property { name, offset, size, field });
            field = mem::read_u64(m, field + l.next).unwrap_or(0);
        }
        out
    }

    /// A class and every class above it, nearest first.
    pub fn lineage(&self, m: &dyn Memory, class: u64) -> Vec<u64> {
        let mut out = Vec::new();
        let mut c = class;
        while mem::plausible(c) && out.len() < MAX_DEPTH && !out.contains(&c) {
            out.push(c);
            c = mem::read_u64(m, c + SUPER).unwrap_or(0);
        }
        out
    }

    /// The names of an object's class and of every class above it, nearest first:
    /// `StoryHero_BP_C`, `CharlieCharacterHero`, …, `Object`.
    pub fn class_names(&self, m: &dyn Memory, obj: u64) -> Vec<String> {
        let Some(class) = mem::read_u64(m, obj + CLASS).filter(|&p| mem::plausible(p)) else { return Vec::new() };
        self.lineage(m, class).into_iter().filter_map(|c| self.object(m, c)).collect()
    }

    /// Is the object an instance of the class named `name`, or of one below it?
    pub fn is_a(&self, m: &dyn Memory, obj: u64, name: &str) -> bool {
        self.class_names(m, obj).iter().any(|n| n == name)
    }

    /// A property by name, declared by the class or inherited.
    pub fn find(&self, m: &dyn Memory, class: u64, name: &str) -> Option<Property> {
        self.lineage(m, class).into_iter().find_map(|c| self.properties(m, c).into_iter().find(|p| p.name == name))
    }

    /// A property of an object, by name: where it is in the object.
    pub fn field(&self, m: &dyn Memory, obj: u64, name: &str) -> Option<Property> {
        let class = mem::read_u64(m, obj + CLASS).filter(|&p| mem::plausible(p))?;
        self.find(m, class, name)
    }

    /// The struct a struct (or array-of-struct inner) property holds: the first pointer
    /// past the FProperty base that leads to a `ScriptStruct`.
    pub fn struct_of(&self, m: &dyn Memory, field: u64) -> Option<u64> {
        (0x70..0xA0).step_by(8).find_map(|o| {
            let p = mem::read_u64(m, field + o).filter(|&p| mem::plausible(p))?;
            (self.class(m, p).as_deref() == Some("ScriptStruct")).then_some(p)
        })
    }

    /// What kind of property a field is: `StructProperty`, `ArrayProperty` … — the name
    /// of its FFieldClass (FField::ClassPrivate, +0x8, whose first member is its FName).
    pub fn field_type(&self, m: &dyn Memory, field: u64) -> Option<String> {
        let fc = mem::read_u64(m, field + 8).filter(|&p| mem::plausible(p))?;
        self.get(m, mem::read_u32(m, fc)?)
    }

    /// An array or set property's inner property: the first pointer past the FProperty
    /// base that is itself a property.
    pub fn inner_of(&self, m: &dyn Memory, field: u64) -> Option<u64> {
        (0x70..0xA0).step_by(8).find_map(|o| {
            let p = mem::read_u64(m, field + o).filter(|&p| mem::plausible(p))?;
            self.field_type(m, p).is_some_and(|t| t.ends_with("Property")).then_some(p)
        })
    }

    /// Where a path of properties leads, from an object into the structs it holds:
    /// `["Player", "Knowledge", "KnownFacts"]` → that field's address and property.
    pub fn path(&self, m: &dyn Memory, obj: u64, path: &[&str]) -> Option<(u64, Property)> {
        let (first, rest) = path.split_first()?;
        let mut p = self.field(m, obj, first)?;
        let mut at = obj + p.offset as u64;
        for name in rest {
            let strukt = self.struct_of(m, p.field)?;
            p = self.find(m, strukt, name)?;
            at += p.offset as u64;
        }
        Some((at, p))
    }

    /// An object-pointer property of an object, followed: `controller.Pawn`.
    pub fn follow(&self, m: &dyn Memory, obj: u64, name: &str) -> Result<u64, String> {
        let p = self.field(m, obj, name).ok_or_else(|| format!("no property {name}"))?;
        if p.size != 8 {
            return Err(format!("{name} is {} bytes, not a pointer", p.size));
        }
        mem::read_u64(m, obj + p.offset as u64).filter(|&v| mem::plausible(v)).ok_or_else(|| format!("{name} is empty"))
    }
}

/// Writable sections of the game image, as (address, size), from its PE headers.
pub fn writable_sections(m: &dyn Memory, base: u64) -> Result<Vec<(u64, u64)>, String> {
    let nt = base + mem::read_u32(m, base + 0x3C).ok_or("image header unreadable")? as u64;
    if mem::read_u32(m, nt) != Some(0x4550) {
        return Err("no PE signature in the game image".into());
    }
    let mut fh = [0u8; 20];
    m.read(nt + 4, &mut fh).then_some(()).ok_or("file header unreadable")?;
    let count = u16::from_le_bytes([fh[2], fh[3]]) as u64;
    let optional = u16::from_le_bytes([fh[16], fh[17]]) as u64;
    let table = nt + 24 + optional;
    let mut out = Vec::new();
    for i in 0..count {
        let mut s = [0u8; 40];
        m.read(table + i * 40, &mut s).then_some(()).ok_or("section table unreadable")?;
        let size = u32::from_le_bytes(s[8..12].try_into().unwrap()) as u64;
        let rva = u32::from_le_bytes(s[12..16].try_into().unwrap()) as u64;
        let flags = u32::from_le_bytes(s[36..40].try_into().unwrap());
        if flags & 0x8000_0000 != 0 {
            out.push((base + rva, size));
        }
    }
    Ok(out)
}

/// Does `block` start with `None` then `ByteProperty`, as block 0 always does?
fn is_block_zero(m: &dyn Memory, block: u64) -> bool {
    let mut b = [0u8; 20];
    m.read(block, &mut b) && &b[2..6] == b"None" && &b[8..20] == b"ByteProperty"
}

/// Find FNamePool, as an RVA. Only a pool that is unique wins.
pub fn discover(m: &dyn Memory, base: u64) -> Result<u64, String> {
    let mut hits = Vec::new();
    let mut buf = vec![0u8; 0x10000];
    for (start, size) in writable_sections(m, base)? {
        let mut at = start;
        while at < start + size {
            let len = (start + size - at).min(buf.len() as u64) as usize;
            let chunk = &mut buf[..len];
            if m.read(at, chunk) {
                let q = |i: usize| u64::from_le_bytes(chunk[i..i + 8].try_into().unwrap());
                // Blocks[0] sits 0x10 into the pool, after CurrentBlock and
                // CurrentByteCursor — both small. Checking those in the buffer first
                // leaves only a handful of candidates worth a read.
                for i in (8..len.saturating_sub(7)).step_by(8) {
                    let (cur_block, cursor) = (q(i - 8) as u32, (q(i - 8) >> 32) as u32);
                    let block0 = q(i);
                    if cur_block < MAX_BLOCKS
                        && cursor < BLOCK_BYTES
                        && mem::plausible(block0)
                        && is_block_zero(m, block0)
                    {
                        hits.push(at + i as u64 - BLOCKS - base);
                    }
                }
            }
            at += len as u64;
        }
    }
    hits.sort_unstable();
    hits.dedup();
    match hits[..] {
        [rva] => Ok(rva),
        [] => Err("no name pool found in the game image".into()),
        _ => Err(format!("{} name pool candidates, refusing to guess: {:X?}", hits.len(), hits)),
    }
}

/// A name pool, classes and their properties, laid out in fake memory the way the
/// game lays them out — for tests above this module.
#[cfg(test)]
pub mod fixture {
    use super::*;
    use crate::mem::fake::Fake;

    pub struct Pool {
        block: u64,
        cursor: u64,
        next_field: u64,
    }

    impl Pool {
        pub fn new(m: &Fake, pool: u64, block: u64) -> Pool {
            m.put(pool, &[0; 0x20]);
            m.ptr(pool + BLOCKS, block);
            let mut p = Pool { block, cursor: 0, next_field: 0x7000_0000 };
            p.name(m, "None");
            p.name(m, "ByteProperty");
            p
        }

        /// Add a name; its FName index.
        pub fn name(&mut self, m: &Fake, s: &str) -> u32 {
            let index = (self.cursor / 2) as u32;
            let mut e = ((s.len() as u16) << 6).to_le_bytes().to_vec();
            e.extend(s.as_bytes());
            if e.len() % 2 == 1 {
                e.push(0);
            }
            m.put(self.block + self.cursor, &e);
            self.cursor += e.len() as u64;
            index
        }

        /// `object` is an instance of a class named `class`, declaring `props` as
        /// (name, offset, element size). Returns the fields, in order.
        pub fn class(
            &mut self,
            m: &Fake,
            object: u64,
            class_obj: u64,
            class: &str,
            props: &[(&str, u32, u32)],
        ) -> Vec<u64> {
            m.put(class_obj, &[0; 0x60]);
            m.ptr(object + CLASS, class_obj);
            let idx = self.name(m, class);
            m.put(class_obj + NAME, &idx.to_le_bytes());
            let mut link = class_obj + CHILD_PROPERTIES;
            let mut fields = Vec::new();
            for &(name, offset, size) in props {
                let f = self.next_field;
                self.next_field += 0x60;
                m.put(f, &[0; 0x60]);
                let idx = self.name(m, name);
                let l = LAYOUTS[0];
                m.put(f + l.name, &idx.to_le_bytes());
                m.put(f + l.size, &size.to_le_bytes());
                m.put(f + l.offset, &offset.to_le_bytes());
                m.ptr(link, f);
                link = f + l.next;
                fields.push(f);
            }
            fields
        }

        /// Make `field` a struct property holding a struct named `name` that declares
        /// `props`. Returns the struct.
        pub fn strukt(&mut self, m: &Fake, field: u64, name: &str, props: &[(&str, u32, u32)]) -> u64 {
            let (strukt, meta, holder) = (self.next_field + 0x100, self.next_field + 0x200, self.next_field + 0x300);
            self.next_field += 0x400;
            // `class` hangs `props` off its class object and names it: here, the struct.
            m.put(holder, &[0; 0x20]);
            self.class(m, holder, strukt, name, props);
            // The struct's own class is one named ScriptStruct.
            m.put(meta, &[0; 0x60]);
            m.ptr(strukt + CLASS, meta);
            let idx = self.name(m, "ScriptStruct");
            m.put(meta + NAME, &idx.to_le_bytes());
            m.ptr(field + 0x78, strukt);
            strukt
        }

        /// The fields declared on `owner` (a class or struct), in order.
        pub fn properties_of(&self, m: &Fake, owner: u64) -> Vec<u64> {
            let l = LAYOUTS[0];
            let mut out = Vec::new();
            let mut f = crate::mem::read_u64(m, owner + CHILD_PROPERTIES).unwrap_or(0);
            while f != 0 {
                out.push(f);
                f = crate::mem::read_u64(m, f + l.next).unwrap_or(0);
            }
            out
        }

        /// `class_obj` derives from `super_obj`.
        pub fn inherit(&self, m: &Fake, class_obj: u64, super_obj: u64) {
            m.ptr(class_obj + SUPER, super_obj);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::fake::Fake;

    const BASE: u64 = 0x1_4000_0000;
    const POOL_RVA: u64 = 0x2000;
    const BLOCK0: u64 = 0x3000_0000;
    const BLOCK1: u64 = 0x3100_0000;

    fn entry(name: &str) -> Vec<u8> {
        let mut e = ((name.len() as u16) << 6 | 0b10_1010).to_le_bytes().to_vec();
        e.extend(name.as_bytes());
        if e.len() % 2 == 1 {
            e.push(0);
        }
        e
    }

    /// A PE image with one writable section holding the pool, and two name blocks.
    fn image() -> Fake {
        let m = Fake::default();
        m.put(BASE, &[0; 0x400]);
        m.put(BASE + 0x3C, &0x80u32.to_le_bytes());
        m.put(BASE + 0x80, &0x4550u32.to_le_bytes());
        let mut fh = [0u8; 20];
        fh[2..4].copy_from_slice(&1u16.to_le_bytes());
        fh[16..18].copy_from_slice(&0xF0u16.to_le_bytes());
        m.put(BASE + 0x84, &fh);
        let mut sec = [0u8; 40];
        sec[8..12].copy_from_slice(&0x1000u32.to_le_bytes());
        sec[12..16].copy_from_slice(&0x2000u32.to_le_bytes());
        sec[36..40].copy_from_slice(&0xC000_0040u32.to_le_bytes());
        m.put(BASE + 0x80 + 24 + 0xF0, &sec);
        m.put(BASE + 0x2000, &[0; 0x1000]);
        m.put(BASE + POOL_RVA + 8, &1u32.to_le_bytes());
        m.ptr(BASE + POOL_RVA + BLOCKS, BLOCK0);
        m.ptr(BASE + POOL_RVA + BLOCKS + 8, BLOCK1);

        let mut b0 = Vec::new();
        for n in ["None", "ByteProperty", "Object"] {
            b0.extend(entry(n));
        }
        m.put(BLOCK0, &b0);
        m.put(BLOCK1, &entry("ATR_Loot_C"));
        m
    }

    #[test]
    fn finds_the_pool_by_its_first_two_names() {
        assert_eq!(discover(&image(), BASE), Ok(POOL_RVA));
    }

    #[test]
    fn decodes_indices_across_blocks() {
        let m = image();
        let n = Names::new(BASE + POOL_RVA);
        assert_eq!(n.get(&m, 0).as_deref(), Some("None"));
        assert_eq!(n.get(&m, 3).as_deref(), Some("ByteProperty")); // 6 bytes in
        assert_eq!(n.get(&m, 1 << 16).as_deref(), Some("ATR_Loot_C"));
        assert_eq!(n.get(&m, 5 << 16), None, "an unused block is not a name");
    }

    #[test]
    fn reads_an_objects_class_name() {
        let m = image();
        let n = Names::new(BASE + POOL_RVA);
        let (obj, class) = (0x5000_0000u64, 0x5100_0000u64);
        m.put(obj, &[0; 0x20]);
        m.put(class, &[0; 0x20]);
        m.ptr(obj + CLASS, class);
        m.put(class + NAME, &(1u32 << 16).to_le_bytes());
        assert_eq!(n.class(&m, obj).as_deref(), Some("ATR_Loot_C"));
    }

    #[test]
    fn walks_a_classs_own_properties() {
        let m = Fake::default();
        let mut pool = fixture::Pool::new(&m, 0x1000_0000, 0x3000_0000);
        pool.class(
            &m,
            0x4000_0000,
            0x5000_0000,
            "ATR_Loot",
            &[("LootingMultiplier", 0x90, 16), ("RarityBonusChance", 0xB0, 16)],
        );
        let n = Names::new(0x1000_0000);
        assert_eq!(n.class(&m, 0x4000_0000).as_deref(), Some("ATR_Loot"));
        let props = n.properties(&m, 0x5000_0000);
        assert_eq!(
            props.iter().map(|p| (p.name.as_str(), p.offset)).collect::<Vec<_>>(),
            [("LootingMultiplier", 0x90), ("RarityBonusChance", 0xB0)]
        );
    }

    #[test]
    fn finds_inherited_properties_and_follows_pointers() {
        let m = Fake::default();
        let mut pool = fixture::Pool::new(&m, 0x1000_0000, 0x3000_0000);
        let (obj, class, parent) = (0x4000_0000u64, 0x5000_0000u64, 0x5100_0000u64);
        pool.class(&m, obj, class, "StoryHero_BP_C", &[("Mesh2", 0x400, 8)]);
        pool.class(&m, 0x4100_0000, parent, "CharlieCharacterHero", &[("Pawnish", 0x300, 8)]);
        pool.inherit(&m, class, parent);
        m.ptr(obj + 0x300, 0x4200_0000);
        let n = Names::new(0x1000_0000);
        assert_eq!(n.class_names(&m, obj), ["StoryHero_BP_C", "CharlieCharacterHero"]);
        assert!(n.is_a(&m, obj, "CharlieCharacterHero"));
        assert_eq!(n.field(&m, obj, "Pawnish").map(|p| p.offset), Some(0x300));
        assert_eq!(n.follow(&m, obj, "Pawnish"), Ok(0x4200_0000));
        assert!(n.follow(&m, obj, "Mesh2").is_err(), "unset pointer");
        assert!(n.follow(&m, obj, "Nope").is_err());
    }

    #[test]
    fn follows_a_path_into_nested_structs() {
        let m = Fake::default();
        let mut pool = fixture::Pool::new(&m, 0x1000_0000, 0x3000_0000);
        let (obj, class) = (0x4000_0000u64, 0x5000_0000u64);
        m.put(obj, &[0; 0x100]);
        let f = pool.class(&m, obj, class, "CharlieSaveGame", &[("Version", 0x38, 4), ("Player", 0x60, 0x5A0)]);
        let player = pool.strukt(&m, f[1], "CharlieSavePlayerState", &[("Knowledge", 0x8, 0x30)]);
        let kf = pool.properties_of(&m, player);
        pool.strukt(&m, kf[0], "CharlieKnowledgeState", &[("KnownFacts", 0x0, 0x10), ("FactTags", 0x10, 0x20)]);
        let n = Names::new(0x1000_0000);
        let (at, p) = n.path(&m, obj, &["Player", "Knowledge", "FactTags"]).unwrap();
        assert_eq!((at, p.name.as_str(), p.size), (obj + 0x60 + 0x8 + 0x10, "FactTags", 0x20));
        assert!(n.path(&m, obj, &["Player", "Nope"]).is_none());
        assert!(n.path(&m, obj, &["Version", "X"]).is_none(), "not a struct");
    }

    #[test]
    fn two_pools_is_no_answer() {
        let m = image();
        m.put(BASE + 0x2800 + 8, &1u32.to_le_bytes());
        m.ptr(BASE + 0x2800 + BLOCKS, BLOCK0);
        assert!(discover(&m, BASE).is_err());
    }
}
