//! The clue board (.spec/JOURNEY.md §3.2): the Datapad's facts the hero knows, grouped
//! by the entry they are about — a person, a place, a thing — in the game's language,
//! so the one clue needed is found without paging through the Datapad.

use std::collections::{BTreeMap, HashSet};

use crate::i18n::names::Fact;

/// One Datapad entry and what the hero knows of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Subject {
    /// The story unit (`VictorGaz`).
    pub unit: String,
    /// Its name as the hero knows it ("Victor Gaz", or "Young man dragging his feet"
    /// before that).
    pub name: String,
    /// The quests it belongs to, by the names the game gives them.
    pub quests: Vec<String>,
    /// What is known of it: description first, then connections, then where.
    pub lines: Vec<String>,
}

impl Subject {
    /// The lines that hold `query` (any case); all of them when the name does.
    pub fn matching(&self, query: &str) -> Vec<&str> {
        let q = query.to_lowercase();
        if self.name.to_lowercase().contains(&q) {
            return self.lines.iter().map(String::as_str).collect();
        }
        self.lines.iter().filter(|l| l.to_lowercase().contains(&q)).map(String::as_str).collect()
    }
}

/// The clue board's material: the entries, the items held by name, and whether the
/// game's fact text was read at all (`doctor locale`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clues {
    pub subjects: Vec<Subject>,
    pub items: Vec<String>,
    pub text: bool,
}

/// The Datapad's order within an entry.
fn rank(category: &str) -> u8 {
    match category {
        "Description" => 0,
        "Connections" => 1,
        "Location" | "WorldZone" => 2,
        _ => 3,
    }
}

/// Every entry the hero knows something about, by name. `fact` gives a known fact's
/// text and entry (i18n `fact`), `name` an entry's name by what the hero knows (i18n
/// `subject`); a quest's own facts (`Quest01`) are its journal entry, not a clue.
pub fn subjects(
    known: &HashSet<String>,
    fact: impl Fn(&str) -> Option<Fact>,
    name: impl Fn(&str) -> Option<String>,
) -> Vec<Subject> {
    let mut by: BTreeMap<String, Vec<(&str, Fact)>> = BTreeMap::new();
    for f in known {
        if let Some(x) = fact(f).filter(|x| !x.unit.starts_with("Quest")) {
            by.entry(x.unit.clone()).or_default().push((f, x));
        }
    }
    let mut out: Vec<Subject> = by
        .into_iter()
        .filter_map(|(unit, mut facts)| {
            facts.sort_by(|a, b| (rank(&a.1.category), a.0).cmp(&(rank(&b.1.category), b.0)));
            let texts = |keep: &dyn Fn(&str) -> bool| {
                let mut v: Vec<String> = Vec::new();
                for (_, x) in facts.iter().filter(|(_, x)| keep(&x.category)) {
                    if !v.contains(&x.text) {
                        v.push(x.text.clone());
                    }
                }
                v
            };
            let lines = texts(&|c| rank(c) < 3);
            if lines.is_empty() {
                return None;
            }
            // A name fact the hero knows, a real one first, when the name table has none.
            let known_name = facts
                .iter()
                .filter(|(_, x)| x.category == "Name")
                .max_by_key(|(f, _)| f.contains("_Real"))
                .map(|(_, x)| x.text.clone());
            Some(Subject {
                name: name(&unit).or(known_name).unwrap_or_else(|| unit.clone()),
                quests: texts(&|c| c == "Quest"),
                lines,
                unit,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(unit: &str, track: &str, category: &str, text: &str) -> Fact {
        Fact { unit: unit.into(), track: track.into(), category: category.into(), text: text.into() }
    }

    fn table(name: &str) -> Option<Fact> {
        Some(match name {
            "Victor_Desc" => f("VictorGaz", "Desc01", "Description", "A young man hiding in the workshop."),
            "Victor_State01_Info01" => f("VictorGaz", "Info01", "Connections", "I found him in the tunnel."),
            "Victor_State01_Info02" => f("VictorGaz", "Info02", "Connections", "He works for Vitalis."),
            "Victor_State02_Info09" => f("VictorGaz", "Info09", "Connections", "He works for Vitalis."),
            "Victor_Quest" => f("VictorGaz", "Quest", "Quest", "Family Reunion"),
            "Victor_Dummy_Name" => f("VictorGaz", "Name", "Name", "Young man"),
            "Victor_Real_Name" => f("VictorGaz", "Name", "Name", "Victor Gaz"),
            "Quest01_Desc" => f("Quest01", "Desc01", "Description", "The quest's own text."),
            "Lucy_Portrait" => f("Lucy", "Name", "Name", "Lucy"),
            _ => return None,
        })
    }

    #[test]
    fn known_facts_make_entries_in_the_datapads_order() {
        let known: HashSet<String> = [
            "Victor_State01_Info02",
            "Victor_Desc",
            "Victor_State01_Info01",
            "Victor_State02_Info09",
            "Victor_Quest",
            "Victor_Dummy_Name",
            "Victor_Real_Name",
            "Quest01_Desc",
            "Lucy_Portrait",
            "Unknown_Fact",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let list = subjects(&known, table, |_| None);
        assert_eq!(list.len(), 1, "a quest's own facts and a name alone are not entries");
        let v = &list[0];
        assert_eq!(v.name, "Victor Gaz", "the real name, known");
        assert_eq!(v.quests, ["Family Reunion"]);
        assert_eq!(
            v.lines,
            ["A young man hiding in the workshop.", "I found him in the tunnel.", "He works for Vitalis."],
            "description first, then connections, each text once"
        );
        assert_eq!(v.matching("VITALIS"), ["He works for Vitalis."]);
        assert_eq!(v.matching("victor").len(), 3, "the name matches every line");
        assert_eq!(subjects(&known, table, |_| Some("V. Gaz".into()))[0].name, "V. Gaz", "the name table wins");
    }
}
