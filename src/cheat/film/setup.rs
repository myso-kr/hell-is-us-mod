//! The filming card's settings, kept in `Mods\\film.txt`, and the defaults they start from.

use super::{Lens, Mode, Recording, Source};

/// The filming card's settings, kept in `Mods\film.txt`: the points taken, where the way comes
/// from, the pace, the camera, its distance, back and forth, the key (F1–F12, 0 none), the routes
/// of points kept by name, and the recordings with the one chosen.
#[derive(Clone, Debug, PartialEq)]
pub struct Setup {
    pub points: Vec<[f32; 3]>,
    pub source: Source,
    pub pace: f32,
    pub lens: Lens,
    pub distance: Option<f32>,
    pub fov: Option<f32>,
    pub repeat: bool,
    /// The key (F8): alone, it starts and stops a take; with Ctrl, it adds where the hero stands
    /// to the points, or takes away the one it stands by — as the map's marker key does.
    pub key: u8,
    /// Who moves what: the mod walking the hero, the player playing, the camera alone.
    pub mode: Mode,
    pub routes: Vec<(String, Vec<[f32; 3]>)>,
    pub recordings: Vec<Recording>,
    pub recording: usize,
}

impl Default for Setup {
    fn default() -> Setup {
        Setup {
            points: Vec::new(),
            source: Source::Guide,
            pace: 0.6,
            lens: Lens::Follow,
            distance: None,
            fov: None,
            repeat: false,
            key: KEY,
            mode: Mode::Walk,
            routes: Vec::new(),
            recordings: Vec::new(),
            recording: 0,
        }
    }
}

/// The key by default (F7 is the game's photo mode).
pub const KEY: u8 = 8;
/// A point taken within this of another takes that one away instead (cm), as the map's marker.
pub const NEAR: f32 = 500.0;

/// From the hero's root to its feet (cm): a point taken where it stands is on the floor, as the
/// 3D map's are.
pub const FEET: f32 = 90.0;

/// The distance (cm) and field of view (degrees) the card offers.
pub const DISTANCE: std::ops::RangeInclusive<f32> = 250.0..=1500.0;
pub const FOV: std::ops::RangeInclusive<f32> = 30.0..=100.0;

impl Setup {
    pub fn path() -> std::path::PathBuf {
        crate::paths::data_dir().join("film.txt")
    }

    pub fn load() -> Setup {
        std::fs::read_to_string(Setup::path()).map(|t| Setup::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = std::fs::write(Setup::path(), self.render());
    }

    pub fn render(&self) -> String {
        let pt = |p: &[f32; 3]| format!("{} {} {}", p[0], p[1], p[2]);
        let mut out = format!(
            "source {}\npace {}\nlens {}\ndistance {}\nfov {}\nrepeat {}\nkey {}\nkeys_version 2\nmode {}\nrecording_chosen {}\n",
            self.source.word(),
            self.pace,
            self.lens.word(),
            self.distance.map_or("none".to_string(), |d| d.to_string()),
            self.fov.map_or("none".to_string(), |d| d.to_string()),
            self.repeat,
            self.key,
            self.mode.word(),
            self.recording
        );
        for p in &self.points {
            out += &format!("point {}\n", pt(p));
        }
        for (name, pts) in &self.routes {
            // the name last, as it may hold spaces
            out += &format!("route {}\n", name.replace('\n', " "));
            for p in pts {
                out += &format!("route_point {}\n", pt(p));
            }
        }
        for r in &self.recordings {
            out += &format!("recording {}\n", r.name.replace('\n', " "));
            for (p, t) in r.points.iter().zip(&r.times) {
                out += &format!("rec {} {t}\n", pt(p));
            }
        }
        out
    }

    pub fn parse(text: &str) -> Setup {
        let mut s = Setup::default();
        let point = |f: &[&str]| -> Option<[f32; 3]> {
            let v: Vec<f32> = f.iter().filter_map(|x| x.parse().ok()).filter(|v: &f32| v.is_finite()).collect();
            (v.len() == 3).then(|| [v[0], v[1], v[2]])
        };
        let mut keys_version = 1;
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            match f[..] {
                ["keys_version", v] => keys_version = v.parse().unwrap_or(1),
                // before recordings: points or the guide's route
                ["use_points", v] => s.source = if v == "true" { Source::Points } else { Source::Guide },
                ["source", v] => s.source = Source::from_word(v).unwrap_or_default(),
                ["recording_chosen", v] => s.recording = v.parse().unwrap_or(0),
                ["recording", ..] => s.recordings.push(Recording {
                    name: line["recording ".len()..].trim().to_string(),
                    points: Vec::new(),
                    times: Vec::new(),
                }),
                ["rec", ..] if f.len() == 5 => {
                    if let (Some(r), Some(p), Ok(t)) = (s.recordings.last_mut(), point(&f[1..4]), f[4].parse::<f32>()) {
                        r.points.push(p);
                        r.times.push(t);
                    }
                }
                ["pace", v] => s.pace = v.parse().unwrap_or(s.pace).clamp(0.2, 1.0),
                ["lens", v] => s.lens = Lens::ALL.into_iter().find(|l| l.word() == v).unwrap_or_default(),
                ["distance", v] => {
                    s.distance = v.parse().ok().map(|d: f32| d.clamp(*DISTANCE.start(), *DISTANCE.end()))
                }
                ["fov", v] => s.fov = v.parse().ok().map(|d: f32| d.clamp(*FOV.start(), *FOV.end())),
                ["repeat", v] => s.repeat = v == "true",
                ["mode", v] => s.mode = Mode::from_word(v).unwrap_or_default(),
                // before modes: the flight's switch
                ["flight", "true"] => s.mode = Mode::Flight,
                ["key", v] => s.key = v.parse().unwrap_or(s.key).min(12),
                ["point", ..] => s.points.extend(point(&f[1..])),
                ["route", ..] => s.routes.push((line["route ".len()..].trim().to_string(), Vec::new())),
                ["route_point", ..] => {
                    if let (Some(r), Some(p)) = (s.routes.last_mut(), point(&f[1..])) {
                        r.1.push(p);
                    }
                }
                _ => {}
            }
        }
        // the first keys (F6 to start, F8 for points) gave way to F8, with Ctrl for points
        if keys_version < 2 && !text.is_empty() {
            s.key = KEY;
        }
        s.recording = s.recording.min(s.recordings.len().saturating_sub(1));
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setup_is_kept() {
        let s = Setup {
            points: vec![[1.0, 2.0, 3.0]],
            source: Source::Recording,
            pace: 0.8,
            lens: Lens::Orbit,
            distance: Some(900.0),
            fov: Some(55.0),
            repeat: true,
            key: 9,
            mode: Mode::Live,
            routes: vec![("bridge at dusk".into(), vec![[4.0, 5.0, 6.0], [7.0, 8.0, 9.0]])],
            recordings: vec![Recording {
                name: "the long way".into(),
                points: vec![[1.0, 1.0, 1.0], [51.0, 1.0, 1.0]],
                times: vec![0.0, 0.5],
            }],
            recording: 0,
        };
        assert_eq!(Setup::parse(&s.render()), s);
        assert_eq!(Setup::parse(""), Setup::default());
        // the first keys move to the new ones
        let old = Setup::parse("key 6\npoint_key 8\npace 0.5\nuse_points true\n");
        assert_eq!(old.source, Source::Points);
        assert_eq!((old.key, old.pace), (KEY, 0.5));
    }
}
