//! Good deeds that fail when a story point passes first, and the order of the act 2
//! keystones (F1, F11 in .spec/ITEMS.md).
//!
//! The game keeps no deadline beside a deed: a later scene simply ends it. The
//! deadlines are a table (`assets/missables.tsv`, from the guides that list the
//! missables); where the story stands is read from the journal — which main quests are
//! begun and done — and each deadline is judged against it: due now, later, or past.

use crate::quests::{Kind, Quest, Status};

/// Story points a deed must be done before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Due {
    /// The Keystone of Grief placed: the end of act 1 (Quest02 done).
    Act1End,
    /// Quest03 (the Keystone of Terror) under way changes Jova.
    Quest03,
    /// The Talju evacuation truck leaves, during Quest03.
    Talju,
    /// Leaving the Ministry of Cultural Primacy the first time, during Quest05.
    Ministry,
    /// The third act 2 keystone placed.
    ThirdKeystone,
}

impl Due {
    fn parse(w: &str) -> Option<Due> {
        Some(match w {
            "act1_end" => Due::Act1End,
            "quest03" => Due::Quest03,
            "talju" => Due::Talju,
            "ministry" => Due::Ministry,
            "third_keystone" => Due::ThirdKeystone,
            _ => return None,
        })
    }

    pub fn label(self) -> String {
        match self {
            Due::Act1End => trf!("1막 끝까지"),
            Due::Quest03 => trf!("{g:Facts_KeystoneTerror/KeystoneTerror_Real_Name} 진행 중 — {g:Facts_VillageOfJova/Name_Real} 변화 전"),
            Due::Talju => trf!("{g:Facts_Shared/Universal_Location_Talju} 대피 트럭 출발 전"),
            Due::Ministry => trf!("{g:Facts_Shared/Universal_Location_LethePropaganda} — 처음 떠나기 전"),
            Due::ThirdKeystone => trf!("세 번째 키스톤 전"),
        }
    }
}

/// Where a deadline stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum When {
    /// The story point can come any time now.
    Now,
    Later,
    /// Already past: the deed has most likely failed.
    Passed,
}

/// One missable deed not yet done, and its deadline.
#[derive(Clone, Debug, PartialEq)]
pub struct Deadline {
    /// The journal key (the deed's GUID in hex), to follow it.
    pub key: String,
    pub title: String,
    pub started: bool,
    pub due: Due,
    pub when: When,
    pub what: String,
}

/// The table: (tag stem, deadline, what to do).
fn table() -> Vec<(String, Due, String)> {
    include_str!("../../assets/missables.tsv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            Some((f.first()?.to_string(), Due::parse(f.get(1)?)?, f.get(2).unwrap_or(&"").to_string()))
        })
        .collect()
}

fn status(journal: &[Quest], key: &str) -> Status {
    journal.iter().find(|q| q.key == key).map_or(Status::NotStarted, |q| q.status)
}

/// Act 2 keystones placed: Quest03 (Terror), Quest04 (Ecstasy), Quest05 (Rage) done.
fn keystones(journal: &[Quest]) -> usize {
    ["Quest03", "Quest04", "Quest05"].iter().filter(|k| status(journal, k) == Status::Completed).count()
}

fn act1_over(journal: &[Quest]) -> bool {
    status(journal, "Quest02") == Status::Completed
        || ["Quest03", "Quest04", "Quest05"].iter().any(|k| status(journal, k) != Status::NotStarted)
}

/// How near a story point is.
pub fn when(due: Due, journal: &[Quest]) -> When {
    let st = |k: &str| status(journal, k);
    match due {
        Due::Act1End if act1_over(journal) => When::Passed,
        Due::Act1End => When::Now,
        Due::Quest03 | Due::Talju if st("Quest03") == Status::Completed => When::Passed,
        Due::Quest03 if act1_over(journal) => When::Now,
        Due::Talju if st("Quest03") == Status::Started => When::Now,
        Due::Quest03 | Due::Talju => When::Later,
        Due::Ministry if st("Quest05") == Status::Completed => When::Passed,
        Due::Ministry if st("Quest05") == Status::Started => When::Now,
        Due::Ministry => When::Later,
        Due::ThirdKeystone => match keystones(journal) {
            3 => When::Passed,
            2 => When::Now,
            _ => When::Later,
        },
    }
}

/// The missable deeds not done or failed yet, soonest first. `deeds` = every good
/// deed the game has: (journal key, title, tag stem).
pub fn deadlines(journal: &[Quest], deeds: &[(String, String, String)]) -> Vec<Deadline> {
    let mut out = Vec::new();
    for (stem, due, what) in table() {
        // Some deeds come in parts sharing a stem (A Light in the Dark 1–4).
        for (key, title, _) in deeds.iter().filter(|(_, _, tags)| tags.ends_with(&stem)) {
            let st = status(journal, key);
            if matches!(st, Status::Completed | Status::Failed) {
                continue;
            }
            out.push(Deadline {
                key: key.clone(),
                title: title.clone(),
                started: st == Status::Started,
                due,
                when: when(due, journal),
                what: crate::i18n::text(&what),
            });
        }
    }
    out.sort_by_key(|d| (d.when, !d.started));
    out
}

/// Act 2 advice, while keystones are left: those left in the order the guides
/// suggest (Terror first — some of its items are only in Talju — then Rage, then
/// Ecstasy), and the deeds due before the next one goes down.
pub fn keystone_advice(journal: &[Quest], deadlines: &[Deadline]) -> Option<(Vec<String>, Vec<String>)> {
    if !act1_over(journal) || keystones(journal) == 3 {
        return None;
    }
    let left: Vec<String> = [
        ("Quest03", trf!("{g:Facts_KeystoneTerror/KeystoneTerror_Real_Name} — {g:Facts_Shared/Universal_Location_Talju} · {g:Facts_Shared/Universal_Location_Marastan} · {g:Facts_Shared/Universal_Location_ArcasSpire}")),
        ("Quest05", trf!("{g:Facts_KeystoneRage/KeystoneRage_Name} — {g:Facts_Shared/Universal_Location_Jeljin} · {g:Facts_Shared/Universal_Location_LethePropaganda} · {g:Facts_Shared/Universal_Location_AurigaMuseum}")),
        ("Quest04", trf!("{g:Facts_KeystoneEcstasy/KeystoneEcstasy_Name} — {g:Facts_Shared/Universal_Location_LetheLibrary} · {g:Facts_Shared/Universal_Location_VyssaHills} · {g:Facts_Shared/Universal_Location_PlainsOfMist}")),
    ]
    .into_iter()
    .filter(|(k, _)| status(journal, k) != Status::Completed)
    .map(|(_, l)| l)
    .collect();
    let before: Vec<String> = deadlines.iter().filter(|d| d.when == When::Now).map(|d| d.title.clone()).collect();
    Some((left, before))
}

/// A tracker line when something is due now.
pub fn alert(deadlines: &[Deadline]) -> Option<String> {
    let now: Vec<&Deadline> = deadlines.iter().filter(|d| d.when == When::Now).collect();
    match now.len() {
        0 => None,
        1 => Some(trf!("마감 임박 선행: {a0} ({a1})", a0 = now[0].title, a1 = now[0].due.label())),
        n => Some(trf!("마감 임박 선행 {n}개 — 퀘스트 탭에서 확인", n = n)),
    }
}

/// Whether a journal entry is a good deed (for the deeds list given to `deadlines`).
pub fn is_deed(q: &Quest) -> bool {
    q.kind == Kind::GoodDeed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn main(n: u32, s: Status) -> Quest {
        Quest {
            key: format!("Quest0{n}"),
            kind: Kind::Main(n),
            name: String::new(),
            detail: String::new(),
            status: s,
            progress: None,
            leads: vec![],
            quest: None,
            tags: None,
        }
    }

    #[test]
    fn every_text_of_the_table_is_translated() {
        let en = crate::i18n::text::Table::load("en");
        for (stem, _, what) in table() {
            assert!(en.get(&what).is_some(), "{stem}: {what:?} has no English line in assets/i18n/en.tsv");
        }
    }

    #[test]
    fn deadlines_follow_the_story() {
        let act1 = [main(1, Status::Completed), main(2, Status::Started)];
        assert_eq!(when(Due::Act1End, &act1), When::Now);
        assert_eq!(when(Due::Quest03, &act1), When::Later);
        let act2 = [main(2, Status::Completed), main(3, Status::Started), main(4, Status::Completed), main(5, Status::Completed)];
        assert_eq!(when(Due::Act1End, &act2), When::Passed);
        assert_eq!(when(Due::Talju, &act2), When::Now);
        assert_eq!(when(Due::ThirdKeystone, &act2), When::Now, "two keystones down: the next is the third");
        assert_eq!(when(Due::Ministry, &act2), When::Passed);
    }

    #[test]
    fn the_table_reads_and_names_known_deeds() {
        let t = table();
        assert!(t.len() >= 9);
        let deeds = vec![("k1".to_string(), "Land of Milk and Honey".to_string(), "Secrets.Facts.SabinianBaby".to_string())];
        let d = deadlines(&[main(2, Status::Started)], &deeds);
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].due, d[0].when, d[0].started), (Due::Act1End, When::Now, false));
    }
}
