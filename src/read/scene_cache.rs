//! The ground and obstacles of each world kept on disk, as an engine keeps its shader
//! cache: `Mods\cache\<world>.bin` (paths.rs).
//!
//! Without it, every run starts with no ground on the maps and no obstacles for the
//! routes until the first pass over the game's objects ends (obstacles.rs). With it,
//! the last run's scene shows as soon as the world is known.
//!
//! The ground is gathered: the game holds only the landscape near the hero, so each
//! pass's heightfields are merged into those kept (the same square replaced, others
//! kept), and the maps show ground the hero has walked even where it is not loaded
//! now. Obstacles are not merged (they can move): the cached ones stand in only until
//! the first pass of the run, which replaces them.
//!
//! The format is little-endian: `HIUS`, the version, then the heightfields (origin,
//! spacing, side, heights) and the obstacles (hull, height range, water). A file of
//! another version, or one that does not read, is ignored and written anew.

use crate::obstacles::{Obstacle, Scene};
use crate::terrain::{Heightfield, Terrain};
use std::collections::HashMap;
use std::path::PathBuf;

const MAGIC: &[u8; 4] = b"HIUS";
const VERSION: u32 = 1;

/// Where `world`'s scene is kept: its name with anything but letters, digits, `_`
/// and `-` made `_`.
pub fn path(world: &str) -> PathBuf {
    let name: String = crate::survey::Survey::world_of(world)
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    crate::paths::data_dir().join("cache").join(format!("{name}.bin"))
}

/// The scene kept for `world`, if there is one that reads.
pub fn load(world: &str) -> Option<Scene> {
    decode(&std::fs::read(path(world)).ok()?)
}

/// Keep `scene` for `world`, off the caller's thread (the file can be a few MB).
pub fn save(world: &str, scene: std::sync::Arc<Scene>) {
    let path = path(world);
    std::thread::spawn(move || {
        let bytes = encode(&scene);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // Written beside, then moved over: a run cut short never leaves half a file.
        let part = path.with_extension("part");
        if std::fs::write(&part, bytes).is_ok() {
            let _ = std::fs::rename(&part, &path);
        }
    });
}

/// A field's square, by its origin to the centimetre.
fn key(f: &Heightfield) -> (i64, i64) {
    (f.origin[0].round() as i64, f.origin[1].round() as i64)
}

/// `kept` with `live` over it: a live field replaces the one kept on its square, and
/// the rest are kept. Also whether anything was added.
pub fn merge(kept: &[Heightfield], live: Vec<Heightfield>) -> (Vec<Heightfield>, bool) {
    let mut by_key: HashMap<(i64, i64), Heightfield> = kept.iter().map(|f| (key(f), f.clone())).collect();
    let before = by_key.len();
    for f in live {
        by_key.insert(key(&f), f);
    }
    let grew = by_key.len() > before;
    let mut out: Vec<Heightfield> = by_key.into_values().collect();
    // In a fixed order, so the same ground makes the same file.
    out.sort_by_key(key);
    (out, grew)
}

fn encode(scene: &Scene) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(MAGIC);
    b.extend_from_slice(&VERSION.to_le_bytes());
    let fields = scene.terrain.fields();
    b.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    for f in fields {
        for v in f.origin.iter().chain(&f.spacing) {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&(f.n as u32).to_le_bytes());
        for z in &f.z {
            b.extend_from_slice(&z.to_le_bytes());
        }
    }
    b.extend_from_slice(&(scene.obstacles.len() as u32).to_le_bytes());
    for o in &scene.obstacles {
        b.extend_from_slice(&(o.hull.len() as u32).to_le_bytes());
        for p in &o.hull {
            b.extend_from_slice(&p[0].to_le_bytes());
            b.extend_from_slice(&p[1].to_le_bytes());
        }
        b.extend_from_slice(&o.zmin.to_le_bytes());
        b.extend_from_slice(&o.zmax.to_le_bytes());
        b.push(o.water as u8);
    }
    b
}

/// Reads through a byte slice; every read fails past the end.
struct Cursor<'a>(&'a [u8]);

impl Cursor<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_at_checked(N)?;
        self.0 = rest;
        head.try_into().ok()
    }
    fn u32(&mut self) -> Option<u32> {
        self.take().map(u32::from_le_bytes)
    }
    fn f32(&mut self) -> Option<f32> {
        self.take().map(f32::from_le_bytes)
    }
    fn f64(&mut self) -> Option<f64> {
        self.take().map(f64::from_le_bytes)
    }
    /// A count, no more than the bytes left could hold at `each` bytes apiece.
    fn count(&mut self, each: usize) -> Option<usize> {
        let n = self.u32()? as usize;
        (n.checked_mul(each)? <= self.0.len()).then_some(n)
    }
}

fn decode(bytes: &[u8]) -> Option<Scene> {
    let mut c = Cursor(bytes);
    (&c.take::<4>()? == MAGIC && c.u32()? == VERSION).then_some(())?;
    let mut fields = Vec::with_capacity(c.count(44)?.min(1 << 16));
    for _ in 0..fields.capacity() {
        let origin = [c.f64()?, c.f64()?, c.f64()?];
        let spacing = [c.f64()?, c.f64()?];
        let n = c.count(0)?;
        let cells = n.checked_mul(n)?;
        (n >= 2 && cells.checked_mul(4)? <= c.0.len()).then_some(())?;
        let z = (0..cells).map(|_| c.f32()).collect::<Option<Vec<f32>>>()?;
        fields.push(Heightfield { origin, spacing, n, z });
    }
    let count = c.count(13)?;
    let mut obstacles = Vec::with_capacity(count);
    for _ in 0..count {
        let points = c.count(8)?;
        let hull = (0..points).map(|_| Some([c.f32()?, c.f32()?])).collect::<Option<Vec<_>>>()?;
        let (zmin, zmax, water) = (c.f32()?, c.f32()?, c.take::<1>()?[0] != 0);
        obstacles.push(Obstacle { hull, zmin, zmax, water });
    }
    c.0.is_empty().then(|| Scene { obstacles, terrain: Terrain::new(fields) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(x: f64, h: f32) -> Heightfield {
        Heightfield { origin: [x, 0.0, 0.0], spacing: [100.0, 100.0], n: 3, z: vec![h; 9] }
    }

    #[test]
    fn a_scene_reads_back_as_written() {
        let water = Obstacle { hull: vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]], zmin: -1.0, zmax: 5.0, water: true };
        let scene = Scene { obstacles: vec![water], terrain: Terrain::new(vec![field(0.0, 1.0), field(200.0, 2.0)]) };
        let back = decode(&encode(&scene)).expect("reads");
        assert_eq!(back.obstacles, scene.obstacles);
        assert_eq!(back.terrain.fields(), scene.terrain.fields());
        assert_eq!(back.terrain.height(250.0, 50.0), Some(2.0));
    }

    #[test]
    fn a_bad_file_is_ignored() {
        let scene = Scene { obstacles: vec![], terrain: Terrain::new(vec![field(0.0, 1.0)]) };
        let bytes = encode(&scene);
        assert!(decode(&bytes[..bytes.len() - 1]).is_none(), "cut short");
        let mut other = bytes.clone();
        other[4] = 9;
        assert!(decode(&other).is_none(), "another version");
        assert!(decode(b"HIUS\x01\0\0\0\xff\xff\xff\xff").is_none(), "a count past the end");
    }

    #[test]
    fn live_ground_replaces_its_square_and_the_rest_is_kept() {
        let (merged, grew) = merge(&[field(0.0, 1.0), field(200.0, 1.0)], vec![field(200.0, 7.0)]);
        assert!(!grew);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1].z[0], 7.0, "the live field won");
        let (merged, grew) = merge(&merged, vec![field(400.0, 3.0)]);
        assert!(grew);
        assert_eq!(merged.len(), 3);
    }
}
