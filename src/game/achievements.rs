//! The game's Steam achievements, read from Steam's own cache (no Steam API):
//! `<Steam>\appcache\stats\UserGameStatsSchema_1620730.bin` — every achievement's API
//! name, its name and description in each language, hidden, and the stat that
//! counts its progress — and `UserGameStats_<account>_1620730.bin` — the unlocked
//! bits, unlock times and stat values. Both are binary KeyValues. Steam writes the
//! user file when the game reports a change (.spec/GUIDE.md §28).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const APP: &str = "1620730";

/// A binary KeyValues node.
#[derive(Clone, Debug, PartialEq)]
pub enum Kv {
    Map(BTreeMap<String, Kv>),
    Str(String),
    Int(i64),
    Float(f32),
}

impl Kv {
    pub fn get(&self, key: &str) -> Option<&Kv> {
        match self {
            Kv::Map(m) => m.get(key),
            _ => None,
        }
    }

    pub fn path(&self, keys: &[&str]) -> Option<&Kv> {
        keys.iter().try_fold(self, |n, k| n.get(k))
    }

    pub fn str(&self) -> Option<&str> {
        match self {
            Kv::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn int(&self) -> Option<i64> {
        match self {
            Kv::Int(i) => Some(*i),
            Kv::Str(s) => s.parse().ok(),
            _ => None,
        }
    }

    pub fn entries(&self) -> impl Iterator<Item = (&String, &Kv)> {
        match self {
            Kv::Map(m) => Some(m.iter()),
            _ => None,
        }
        .into_iter()
        .flatten()
    }
}

/// Parse binary KeyValues: type byte, NUL-terminated name, value; 8 ends a map.
pub fn parse(b: &[u8]) -> Option<Kv> {
    fn map(b: &[u8], i: &mut usize) -> Option<BTreeMap<String, Kv>> {
        let mut out = BTreeMap::new();
        while *i < b.len() {
            let t = b[*i];
            *i += 1;
            if t == 8 {
                return Some(out);
            }
            let name = cstr(b, i)?;
            let mut take = |n: usize| -> Option<&[u8]> {
                let s = b.get(*i..*i + n)?;
                *i += n;
                Some(s)
            };
            let v = match t {
                0 => Kv::Map(map(b, i)?),
                1 => Kv::Str(cstr(b, i)?),
                2 => Kv::Int(i32::from_le_bytes(take(4)?.try_into().ok()?) as i64),
                3 => Kv::Float(f32::from_le_bytes(take(4)?.try_into().ok()?)),
                7 => Kv::Int(u64::from_le_bytes(take(8)?.try_into().ok()?) as i64),
                _ => return None,
            };
            out.insert(name, v);
        }
        Some(out)
    }
    fn cstr(b: &[u8], i: &mut usize) -> Option<String> {
        let end = b[*i..].iter().position(|&c| c == 0)? + *i;
        let s = String::from_utf8_lossy(&b[*i..end]).into_owned();
        *i = end + 1;
        Some(s)
    }
    let mut i = 0;
    map(b, &mut i).map(Kv::Map)
}

/// Steam's word for a game culture.
pub fn steam_language(culture: &str) -> &'static str {
    match culture {
        "ko" => "koreana",
        "ja" => "japanese",
        "zh-Hans" => "schinese",
        "de" => "german",
        "fr" => "french",
        "es" => "spanish",
        "it" => "italian",
        "pl" => "polish",
        "pt-BR" => "brazilian",
        "ru" => "russian",
        "tr" => "turkish",
        _ => "english",
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Achievement {
    /// The API name (`Act01_Complete`).
    pub api: String,
    pub name: String,
    pub desc: String,
    pub hidden: bool,
    pub unlocked: bool,
    /// When (Unix seconds), if Steam keeps it.
    pub time: Option<i64>,
    /// (value, of) when a stat counts it.
    pub progress: Option<(i64, i64)>,
}

/// The stats folder: `<Steam>\appcache\stats`.
pub fn stats_dir() -> Option<PathBuf> {
    let d = super::locate::steam_path()?.join("appcache").join("stats");
    d.is_dir().then_some(d)
}

/// The newest user stats file of the game.
fn user_file(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.starts_with("UserGameStats_") && n.ends_with(&format!("_{APP}.bin"))
        })
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

pub fn load(culture: &str) -> Vec<Achievement> {
    let Some(dir) = stats_dir() else { return Vec::new() };
    let schema = std::fs::read(dir.join(format!("UserGameStatsSchema_{APP}.bin"))).ok().and_then(|b| parse(&b));
    let user = user_file(&dir).and_then(|p| std::fs::read(p).ok()).and_then(|b| parse(&b));
    match schema {
        Some(s) => read(&s, user.as_ref(), steam_language(culture)),
        None => Vec::new(),
    }
}

/// The achievements of a schema, against a user file, in a Steam language.
pub fn read(schema: &Kv, user: Option<&Kv>, lang: &str) -> Vec<Achievement> {
    let Some(stats) = schema.path(&[APP, "stats"]) else { return Vec::new() };
    let cache = user.and_then(|u| u.get("cache"));
    // INT stats by API name: their values.
    let mut values: BTreeMap<&str, i64> = BTreeMap::new();
    for (id, st) in stats.entries() {
        if let (Some(name), None) = (st.get("name").and_then(Kv::str), st.get("bits")) {
            let v = cache.and_then(|c| c.path(&[id, "data"])).and_then(Kv::int).unwrap_or(0);
            values.insert(name, v);
        }
    }
    let text = |d: Option<&Kv>| -> String {
        d.and_then(|d| d.get(lang).or_else(|| d.get("english"))).and_then(Kv::str).unwrap_or_default().replace('\u{200b}', "")
    };
    let mut out = Vec::new();
    let mut ids: Vec<(&String, &Kv)> = stats.entries().filter(|(_, s)| s.get("bits").is_some()).collect();
    ids.sort_by_key(|(id, _)| id.parse::<u32>().unwrap_or(u32::MAX));
    for (id, st) in ids {
        let bits = cache.and_then(|c| c.path(&[id, "data"])).and_then(Kv::int).unwrap_or(0) as u64;
        let times = cache.and_then(|c| c.path(&[id, "AchievementTimes"]));
        let mut list: Vec<(&String, &Kv)> = st.get("bits").unwrap().entries().collect();
        list.sort_by_key(|(b, _)| b.parse::<u32>().unwrap_or(u32::MAX));
        for (bit, a) in list {
            let n: u32 = bit.parse().unwrap_or(0);
            let progress = a.get("progress").and_then(|p| {
                let of = p.get("max_val").and_then(Kv::int)?;
                let stat = p.path(&["value", "operand1"]).and_then(Kv::str)?;
                Some((values.get(stat).copied().unwrap_or(0), of))
            });
            out.push(Achievement {
                api: a.get("name").and_then(Kv::str).unwrap_or_default().to_string(),
                name: text(a.path(&["display", "name"])),
                desc: text(a.path(&["display", "desc"])),
                hidden: a.path(&["display", "hidden"]).and_then(Kv::int).unwrap_or(0) != 0,
                unlocked: n < 64 && bits >> n & 1 == 1,
                time: times.and_then(|t| t.get(bit)).and_then(Kv::int),
                progress,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Binary KeyValues writer, for the test.
    fn kv(out: &mut Vec<u8>, name: &str, v: &Kv) {
        let head = |out: &mut Vec<u8>, t: u8| {
            out.push(t);
            out.extend(name.as_bytes());
            out.push(0);
        };
        match v {
            Kv::Map(m) => {
                head(out, 0);
                for (k, x) in m {
                    kv(out, k, x);
                }
                out.push(8);
            }
            Kv::Str(s) => {
                head(out, 1);
                out.extend(s.as_bytes());
                out.push(0);
            }
            Kv::Int(i) => {
                head(out, 2);
                out.extend((*i as i32).to_le_bytes());
            }
            Kv::Float(f) => {
                head(out, 3);
                out.extend(f.to_le_bytes());
            }
        }
    }

    fn map(items: &[(&str, Kv)]) -> Kv {
        Kv::Map(items.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
    }
    fn s(x: &str) -> Kv {
        Kv::Str(x.into())
    }

    #[test]
    fn reads_names_unlocks_and_progress() {
        let ach = |api: &str, en: &str, ko: &str, progress: Option<Kv>| {
            let mut a = vec![
                ("name", s(api)),
                ("display", map(&[("name", map(&[("english", s(en)), ("koreana", s(ko))])), ("desc", map(&[("english", s("d"))])), ("hidden", Kv::Int(0))])),
            ];
            if let Some(p) = progress {
                a.push(("progress", p));
            }
            map(&a)
        };
        let schema = map(&[(
            APP,
            map(&[(
                "stats",
                map(&[
                    (
                        "1",
                        map(&[(
                            "bits",
                            map(&[
                                ("0", ach("Act01_Complete", "So It Begins", "시작이군", None)),
                                (
                                    "1",
                                    ach(
                                        "GoodDeeds",
                                        "Deeds",
                                        "선행",
                                        Some(map(&[("max_val", Kv::Int(26)), ("value", map(&[("operand1", s("DEEDS_STAT"))]))])),
                                    ),
                                ),
                            ]),
                        )]),
                    ),
                    ("3", map(&[("name", s("DEEDS_STAT")), ("type", s("INT"))])),
                ]),
            )]),
        )]);
        let user = map(&[("cache", map(&[("1", map(&[("data", Kv::Int(1)), ("AchievementTimes", map(&[("0", Kv::Int(99))]))])), ("3", map(&[("data", Kv::Int(2))]))]))]);
        // Round trip through the binary form.
        let (mut b, mut u) = (Vec::new(), Vec::new());
        for (k, v) in schema.entries() {
            kv(&mut b, k, v);
        }
        b.push(8);
        for (k, v) in user.entries() {
            kv(&mut u, k, v);
        }
        u.push(8);
        let (schema, user) = (parse(&b).unwrap(), parse(&u).unwrap());
        let a = read(&schema, Some(&user), "koreana");
        assert_eq!(a.len(), 2);
        assert_eq!((a[0].name.as_str(), a[0].unlocked, a[0].time), ("시작이군", true, Some(99)));
        assert_eq!((a[1].unlocked, a[1].progress), (false, Some((2, 26))));
        assert_eq!(read(&schema, None, "german")[0].name, "So It Begins", "English when the language is missing");
    }
}
