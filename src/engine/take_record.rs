//! A take's originals kept on disk while it rolls (`Mods\film_take.txt`): what the camera held
//! before it, and — for a flight, which carries the hero off and shrinks it — where the hero
//! stood and its size. A take that ends puts them back itself and the record goes; a record found
//! with no take rolling was left by a panel that ended mid-take, and is put back on the next step
//! (or by `hiumod restore`). The hero is moved back only while its mesh is still shrunk: the
//! surest sign the take never put it back, and no reason to pull a hero from where it has since
//! walked.

use super::attached::Attached;
use super::camera::{CameraAt, TRANSFORM};
use crate::mem::Memory;

fn path() -> std::path::PathBuf {
    crate::paths::data_dir().join("film_take.txt")
}

/// A mesh smaller than this was shrunk by a flight.
const SHRUNK: f64 = 0.01;

#[derive(Debug, Default, PartialEq)]
pub(super) struct Record {
    /// Each config's distance and field of view.
    configs: Vec<(String, Option<f32>, Option<f32>)>,
    pivot: Option<Vec<u8>>,
    safety: Option<u8>,
    blend: Option<(f32, f32)>,
    /// A flight's: where the hero's root stood, and its mesh's scale.
    hero: Option<[f64; 3]>,
    scale: Option<[f64; 3]>,
}

fn f32_at(m: &dyn Memory, at: u64) -> Option<f32> {
    let mut b = [0u8; 4];
    m.read(at, &mut b).then(|| f32::from_le_bytes(b)).filter(|v| v.is_finite())
}

fn f64x3_at(m: &dyn Memory, at: u64) -> Option<[f64; 3]> {
    let mut b = [0u8; 24];
    m.read(at, &mut b)
        .then(|| [0, 1, 2].map(|i| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap())))
        .filter(|v| v.iter().all(|x| x.is_finite()))
}

/// Where the hero's mesh keeps its scale.
fn scale_at(a: &Attached) -> Option<u64> {
    let (m, n) = (&a.game, &a.anchors.names);
    let hero = a.chain().ok()?.hero(m, &a.anchors).ok()?;
    let mesh = n.follow(m, hero, "Mesh").ok()?;
    n.field(m, mesh, "RelativeScale3D").map(|p| mesh + p.offset as u64)
}

impl Record {
    /// What a take is about to change, read now; `flight` when it carries the hero.
    pub fn capture(a: &Attached, flight: bool) -> Option<Record> {
        let m = &a.game;
        let cam = CameraAt::find(a).ok()?;
        let mut r = Record {
            configs: cam
                .configs
                .iter()
                .map(|(n, d, f)| (n.to_string(), d.and_then(|d| f32_at(m, d)), f.and_then(|f| f32_at(m, f))))
                .collect(),
            ..Default::default()
        };
        r.pivot = cam.pivot.and_then(|p| {
            let mut b = vec![0u8; TRANSFORM];
            m.read(p, &mut b).then_some(b)
        });
        r.safety = cam.safety.and_then(|s| {
            let mut b = [0u8; 1];
            m.read(s, &mut b).then_some(b[0])
        });
        r.blend = cam.blend.and_then(|(i, o)| f32_at(m, i).zip(f32_at(m, o)));
        if flight {
            r.hero = a.pose().ok().map(|(p, _)| p);
            r.scale = scale_at(a).and_then(|s| f64x3_at(m, s));
        }
        Some(r)
    }

    pub fn save(&self) {
        let _ = std::fs::write(path(), self.render());
    }

    fn render(&self) -> String {
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        let mut out = String::new();
        for (name, d, f) in &self.configs {
            let v = |x: &Option<f32>| x.map_or("-".to_string(), |x| x.to_string());
            out += &format!("config {name} {} {}\n", v(d), v(f));
        }
        if let Some(p) = &self.pivot {
            out += &format!("pivot {}\n", hex(p));
        }
        if let Some(s) = self.safety {
            out += &format!("safety {s}\n");
        }
        if let Some((i, o)) = self.blend {
            out += &format!("blend {i} {o}\n");
        }
        if let Some(h) = self.hero {
            out += &format!("hero {} {} {}\n", h[0], h[1], h[2]);
        }
        if let Some(s) = self.scale {
            out += &format!("scale {} {} {}\n", s[0], s[1], s[2]);
        }
        out
    }

    pub fn parse(text: &str) -> Record {
        let mut r = Record::default();
        let three = |f: &[&str]| -> Option<[f64; 3]> {
            let v: Vec<f64> = f.iter().filter_map(|x| x.parse().ok()).collect();
            (v.len() == 3).then(|| [v[0], v[1], v[2]])
        };
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            match f[..] {
                ["config", name, d, fov] => r.configs.push((name.to_string(), d.parse().ok(), fov.parse().ok())),
                ["pivot", hex] if hex.len() == TRANSFORM * 2 => {
                    r.pivot = (0..TRANSFORM).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()).collect();
                }
                ["safety", s] => r.safety = s.parse().ok(),
                ["blend", i, o] => r.blend = i.parse().ok().zip(o.parse().ok()),
                ["hero", ..] => r.hero = three(&f[1..]),
                ["scale", ..] => r.scale = three(&f[1..]),
                _ => {}
            }
        }
        r
    }

    pub fn load() -> Option<Record> {
        std::fs::read_to_string(path()).ok().map(|t| Record::parse(&t))
    }

    pub fn forget() {
        let _ = std::fs::remove_file(path());
    }

    /// Everything put back as it was; the hero moved back only while it is still shrunk. What
    /// was done, for the log.
    pub fn put_back(&self, a: &Attached) -> String {
        let m = &a.game;
        let mut done = Vec::new();
        if let Ok(cam) = CameraAt::find(a) {
            for (name, d_at, f_at) in &cam.configs {
                let Some((_, d, f)) = self.configs.iter().find(|(n, _, _)| n == name) else { continue };
                if let (Some(at), Some(v)) = (d_at, d) {
                    m.write(*at, &v.to_le_bytes());
                }
                if let (Some(at), Some(v)) = (f_at, f) {
                    m.write(*at, &v.to_le_bytes());
                }
            }
            if let (Some(at), Some(b)) = (cam.pivot, &self.pivot) {
                m.write(at, b);
            }
            if let (Some(at), Some(s)) = (cam.safety, self.safety) {
                m.write(at, &[s]);
            }
            if let (Some((i, o)), Some((vi, vo))) = (cam.blend, self.blend) {
                m.write(i, &vi.to_le_bytes());
                m.write(o, &vo.to_le_bytes());
            }
            done.push("the camera");
        }
        let shrunk =
            scale_at(a).and_then(|s| f64x3_at(m, s).map(|v| (s, v))).filter(|(_, v)| v.iter().all(|x| *x < SHRUNK));
        if let (Some((at, _)), Some(s)) = (shrunk, self.scale) {
            let b: Vec<u8> = s.iter().flat_map(|v| v.to_le_bytes()).collect();
            m.write(at, &b);
            done.push("the hero's size");
            if let Some(h) = self.hero {
                if a.teleport([h[0], h[1], h[2] + super::ABOVE]).is_ok() {
                    done.push("the hero where it stood");
                }
            }
        }
        done.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_is_kept() {
        let r = Record {
            configs: vec![
                ("ExplorationConfig".into(), Some(484.0), Some(70.0)),
                ("APCConfig".into(), None, Some(70.0)),
            ],
            pivot: Some((0..TRANSFORM as u8).collect()),
            safety: Some(7),
            blend: Some((0.15, 0.25)),
            hero: Some([1.5, -2.0, 3.25]),
            scale: Some([1.0, 1.0, 1.0]),
        };
        assert_eq!(Record::parse(&r.render()), r);
        assert_eq!(Record::parse(""), Record::default());
    }
}
