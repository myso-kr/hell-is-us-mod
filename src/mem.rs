//! Reading another process's memory and walking pointer chains through it.
//!
//! `Memory` is a trait so the chain walker, and everything built on it, can be tested
//! against a map of addresses instead of a running game.

pub trait Memory {
    /// Fill `buf` from `addr`. False unless every byte was read.
    fn read(&self, addr: u64, buf: &mut [u8]) -> bool;
    /// Write all of `data` at `addr`. False unless every byte was written.
    fn write(&self, addr: u64, data: &[u8]) -> bool;
}

pub fn read_u64(m: &dyn Memory, addr: u64) -> Option<u64> {
    let mut b = [0u8; 8];
    m.read(addr, &mut b).then(|| u64::from_le_bytes(b))
}

pub fn read_u32(m: &dyn Memory, addr: u64) -> Option<u32> {
    let mut b = [0u8; 4];
    m.read(addr, &mut b).then(|| u32::from_le_bytes(b))
}

pub fn read_f32(m: &dyn Memory, addr: u64) -> Option<f32> {
    let mut b = [0u8; 4];
    m.read(addr, &mut b).then(|| f32::from_le_bytes(b))
}

pub fn write_f32(m: &dyn Memory, addr: u64, v: f32) -> bool {
    m.write(addr, &v.to_le_bytes())
}

/// A user-mode pointer that could point at a heap object. Anything below 64 KiB is a
/// null-ish value or a small integer, and anything above 0x7FFF_FFFF_FFFF is not a
/// user-mode address on x64 Windows.
pub fn plausible(ptr: u64) -> bool {
    (0x10000..0x8000_0000_0000).contains(&ptr)
}

#[derive(Debug, PartialEq)]
pub struct ChainError {
    /// Which offset the walk was about to apply when it stopped.
    pub step: usize,
    pub addr: u64,
    pub why: &'static str,
}

impl std::fmt::Display for ChainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pointer chain broke at step {} (0x{:X}): {}", self.step, self.addr, self.why)
    }
}

/// Cheat Engine's pointer shape: read a pointer at `start`, add the first offset, read
/// a pointer there, add the next, and so on. Returns the address after the last
/// offset — not dereferenced.
pub fn resolve(m: &dyn Memory, start: u64, offsets: &[u64]) -> Result<u64, ChainError> {
    let mut addr = start;
    for (step, off) in offsets.iter().enumerate() {
        let ptr = read_u64(m, addr).ok_or(ChainError { step, addr, why: "unreadable" })?;
        if !plausible(ptr) {
            return Err(ChainError { step, addr, why: "not a pointer (not loaded in yet?)" });
        }
        addr = ptr + off;
    }
    Ok(addr)
}

#[cfg(test)]
pub mod fake {
    use super::Memory;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// Byte-addressed memory for tests. Unset bytes are unreadable.
    #[derive(Default)]
    pub struct Fake(pub RefCell<HashMap<u64, u8>>);

    impl Fake {
        pub fn put(&self, addr: u64, bytes: &[u8]) {
            let mut m = self.0.borrow_mut();
            for (i, b) in bytes.iter().enumerate() {
                m.insert(addr + i as u64, *b);
            }
        }
        pub fn ptr(&self, addr: u64, v: u64) {
            self.put(addr, &v.to_le_bytes())
        }
        pub fn f32(&self, addr: u64, v: f32) {
            self.put(addr, &v.to_le_bytes())
        }
    }

    impl Memory for Fake {
        fn read(&self, addr: u64, buf: &mut [u8]) -> bool {
            let m = self.0.borrow();
            for (i, b) in buf.iter_mut().enumerate() {
                match m.get(&(addr + i as u64)) {
                    Some(v) => *b = *v,
                    None => return false,
                }
            }
            true
        }
        fn write(&self, addr: u64, data: &[u8]) -> bool {
            if !self.read(addr, &mut vec![0; data.len()]) {
                return false;
            }
            self.put(addr, data);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::Fake;
    use super::*;

    #[test]
    fn walks_each_offset_after_a_dereference() {
        let m = Fake::default();
        m.ptr(0x1000_0000, 0x2000_0000);
        m.ptr(0x2000_0010, 0x3000_0000);
        assert_eq!(resolve(&m, 0x1000_0000, &[0x10, 0x20]), Ok(0x3000_0020));
    }

    #[test]
    fn a_null_link_names_its_step() {
        let m = Fake::default();
        m.ptr(0x1000_0000, 0x2000_0000);
        m.ptr(0x2000_0010, 0);
        let e = resolve(&m, 0x1000_0000, &[0x10, 0x20]).unwrap_err();
        assert_eq!((e.step, e.addr), (1, 0x2000_0010));
    }

    #[test]
    fn an_unmapped_link_is_an_error_not_a_zero() {
        let m = Fake::default();
        assert_eq!(resolve(&m, 0x1000_0000, &[0]).unwrap_err().why, "unreadable");
    }
}
