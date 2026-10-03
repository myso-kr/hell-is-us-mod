//! The game's names in the game's language, from its own translations:
//! `Mods\locale\<culture>.tsv` (namespace, key, text — every line of the game's locres)
//! and `Mods\locale\names.tsv` (which namespace and key names an item, an NPC, a
//! region). Both are written by `doctor locale`; without them every lookup is `None`
//! and the mod falls back to names made from class names.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub fn dir() -> PathBuf {
    crate::paths::data_dir().join("locale")
}

#[derive(Default)]
pub struct Names {
    /// Item data asset name (lower case) → name.
    items: HashMap<String, String>,
    regions: HashMap<String, String>,
    /// NPC class (lower case) → (the name met by, its story unit).
    npcs: HashMap<String, (String, String)>,
    /// Story unit → (name fact, name) of each name the hero may learn.
    learned: HashMap<String, Vec<(String, String)>>,
    /// English source text → this language's, for the texts the game hands over as
    /// their string table's source until it has shown them once (good deed titles,
    /// places): the namespaces `sourced` names.
    sources: HashMap<String, String>,
    /// Every text of the game in this language, by (namespace, key): what a
    /// `{g:namespace/key}` in the mod's words names.
    game: HashMap<(String, String), String>,
}

/// The namespaces whose texts are looked up by their English source.
fn sourced(ns: &str) -> bool {
    ns == "UI_Secrets" || ns.starts_with("Facts_")
}

impl Names {
    pub fn load(culture: &str) -> Names {
        Names::read(&dir(), culture).unwrap_or_default()
    }

    pub fn read(dir: &Path, culture: &str) -> Option<Names> {
        let file = super::culture::best(culture, |c| dir.join(format!("{c}.tsv")).is_file())?;
        let text = std::fs::read_to_string(dir.join(format!("{file}.tsv"))).ok()?;
        let index = std::fs::read_to_string(dir.join("names.tsv")).ok()?;
        let mut n = Names::parse(&text, &index);
        n.game = text
            .lines()
            .filter_map(|l| {
                let mut p = l.splitn(3, '\t');
                Some(((p.next()?.to_string(), p.next()?.to_string()), unescape(p.next()?)))
            })
            .collect();
        if let Ok(en) = std::fs::read_to_string(dir.join("en.tsv")) {
            n.add_sources(&en, &text);
        }
        Some(n)
    }

    /// Pair the English text with this language's, line by line of the same key.
    pub fn add_sources(&mut self, en: &str, text: &str) {
        let mut ours: HashMap<(&str, &str), &str> = HashMap::new();
        for l in text.lines() {
            let mut p = l.splitn(3, '\t');
            if let (Some(ns), Some(key), Some(t)) = (p.next(), p.next(), p.next()) {
                if sourced(ns) {
                    ours.insert((ns, key), t);
                }
            }
        }
        for l in en.lines() {
            let mut p = l.splitn(3, '\t');
            let (Some(ns), Some(key), Some(t)) = (p.next(), p.next(), p.next()) else { continue };
            if let Some(o) = ours.get(&(ns, key)).filter(|o| !o.is_empty()) {
                self.sources.entry(unescape(t)).or_insert_with(|| unescape(o));
            }
        }
    }

    /// One of the game's texts in this language.
    pub fn game_text(&self, ns: &str, key: &str) -> Option<String> {
        self.game.get(&(ns.to_string(), key.to_string())).filter(|t| !t.is_empty()).cloned()
    }

    /// The text in this language for one the game gave in English (its source).
    pub fn from_source(&self, english: &str) -> Option<String> {
        self.sources.get(english).cloned()
    }

    pub fn parse(text: &str, index: &str) -> Names {
        // Only the lines the index names are kept: a few hundred of the thousands.
        let wanted: HashSet<(&str, &str)> = index
            .lines()
            .filter_map(|l| {
                let mut p = l.split('\t').skip(2);
                Some((p.next()?, p.next()?))
            })
            .collect();
        let mut texts: HashMap<(&str, &str), String> = HashMap::new();
        for l in text.lines() {
            let mut p = l.splitn(3, '\t');
            let (Some(ns), Some(key), Some(t)) = (p.next(), p.next(), p.next()) else { continue };
            if wanted.contains(&(ns, key)) {
                texts.insert((ns, key), unescape(t));
            }
        }
        let mut n = Names::default();
        for l in index.lines() {
            let p: Vec<&str> = l.split('\t').collect();
            let [kind, id, ns, key, rest @ ..] = p.as_slice() else { continue };
            let Some(t) = texts.get(&(*ns, *key)).filter(|t| !t.is_empty()).cloned() else { continue };
            let unit = rest.first().copied().unwrap_or("-").to_string();
            match *kind {
                "item" => {
                    n.items.insert(id.to_string(), t);
                }
                "region" => {
                    n.regions.insert(id.to_string(), t);
                }
                "npc" => {
                    n.npcs.insert(id.to_string(), (t, unit));
                }
                "name" => n.learned.entry(unit).or_default().push((id.to_string(), t)),
                _ => {}
            }
        }
        n
    }

    pub fn len(&self) -> usize {
        self.items.len() + self.regions.len() + self.npcs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// By the data asset's name, or a path ending in it (`/Game/Items/…/X_Item_DA` or
    /// `…X_Item_DA.X_Item_DA`).
    pub fn item(&self, asset: &str) -> Option<String> {
        let last = asset.rsplit('/').next()?.split('.').next()?;
        self.items.get(&last.to_lowercase()).cloned()
    }

    pub fn region(&self, world: &str) -> Option<String> {
        self.regions.get(world).cloned()
    }

    /// The name the hero knows the NPC by: a real name a known fact of their story
    /// unit told (the later one, by the fact's name, when several), else the name
    /// they are met by.
    pub fn npc(&self, class: &str, facts: &HashSet<String>) -> Option<String> {
        let (met, unit) = self.npcs.get(&class.to_lowercase())?;
        let learned = self
            .learned
            .get(unit)
            .into_iter()
            .flatten()
            .filter(|(fact, _)| facts.contains(fact) && !placeholder(fact))
            .max_by_key(|(fact, _)| (rank(fact), fact.clone()));
        Some(learned.map_or(met, |(_, t)| t).clone())
    }

    /// A story unit's name — the subject of a quest's clues (`Quest01_Tania` is about
    /// `Tania`): the most telling name the hero knows (a real one over a nickname);
    /// knowing none, the placeholder the game shows first ("Mysterious woman"), so a
    /// real name is not given away; a unit with only a real name (a place) shows it.
    pub fn subject(&self, unit: &str, facts: &HashSet<String>) -> Option<String> {
        let names = self.learned.get(unit)?;
        let best = |list: &mut dyn Iterator<Item = &(String, String)>| {
            list.max_by_key(|(f, _)| (rank(f), f.clone())).map(|(_, t)| t.clone())
        };
        best(&mut names.iter().filter(|(f, _)| facts.contains(f)))
            .or_else(|| best(&mut names.iter().filter(|(f, _)| placeholder(f))))
            .or_else(|| (names.len() == 1).then(|| names[0].1.clone()))
    }
}

/// A name fact the game shows before the real name is learned.
fn placeholder(fact: &str) -> bool {
    fact.contains("Dummy") || fact.contains("Unknown")
}

/// How telling a name fact is: a real name, then any other, then a placeholder.
fn rank(fact: &str) -> u8 {
    match (placeholder(fact), fact.contains("_Real")) {
        (true, _) => 0,
        (false, false) => 1,
        (false, true) => 2,
    }
}

/// The text as written in the table, its escapes undone — and without the zero-width
/// spaces the Japanese text carries for line breaking (a box in some fonts).
fn unescape(s: &str) -> String {
    let s = &s.replace('\u{200b}', "");
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        match (c, c == '\\') {
            (_, false) => out.push(c),
            _ => match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => {}
                Some(o) => out.push(o),
                None => {}
            },
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "Facts_Shared\tUniversal_Location_Talju\t시험 마을\n\
        Facts_TaljuMechanic\tTaljuMechanic_Name_Unknown\t낯선 사람\n\
        Facts_TaljuMechanic\tTaljuMechanic_Name_Real\t홍길동\n\
        Secrets_Items\tTaljuProfessorHouseKey_Name\t시험 열쇠\n\
        Other\tLine\tnot named\\nby anything\n";
    const INDEX: &str = "item\ttaljuprofessorhousekey_item_da\tSecrets_Items\tTaljuProfessorHouseKey_Name\n\
        region\tTalju\tFacts_Shared\tUniversal_Location_Talju\n\
        npc\tconvo_mechanicsurvivor_bp_c\tFacts_TaljuMechanic\tTaljuMechanic_Name_Unknown\tTaljuMechanic\n\
        name\tTaljuMechanic_Dummy_Name_TextFact_DA\tFacts_TaljuMechanic\tTaljuMechanic_Name_Unknown\tTaljuMechanic\n\
        name\tTaljuMechanic_Real_Name_TextFact_DA\tFacts_TaljuMechanic\tTaljuMechanic_Name_Real\tTaljuMechanic\n";

    #[test]
    fn names_items_regions_and_npcs() {
        let n = Names::parse(TEXT, INDEX);
        assert_eq!(n.item("/Game/Items/Secrets/Talju/TaljuProfessorHouseKey_item_DA").as_deref(), Some("시험 열쇠"));
        assert_eq!(
            n.item("TaljuProfessorHouseKey_Item_DA.TaljuProfessorHouseKey_Item_DA").as_deref(),
            Some("시험 열쇠")
        );
        assert_eq!(n.region("Talju").as_deref(), Some("시험 마을"));
        let mut facts = HashSet::new();
        assert_eq!(n.npc("Convo_MechanicSurvivor_BP_C", &facts).as_deref(), Some("낯선 사람"));
        facts.insert("TaljuMechanic_Real_Name_TextFact_DA".to_string());
        assert_eq!(
            n.npc("Convo_MechanicSurvivor_BP_C", &facts).as_deref(),
            Some("홍길동"),
            "the real name, once learned"
        );
        assert_eq!(n.npc("Nobody_BP_C", &facts), None);
        assert_eq!(n.subject("TaljuMechanic", &facts).as_deref(), Some("홍길동"), "known: the real name");
        assert_eq!(
            n.subject("TaljuMechanic", &HashSet::new()).as_deref(),
            Some("낯선 사람"),
            "unknown: the placeholder"
        );
        assert_eq!(n.subject("Nowhere", &facts), None);
        assert_eq!(n.len(), 3);
    }
}
