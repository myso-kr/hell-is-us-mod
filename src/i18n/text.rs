//! The mod's own words: `assets/i18n/<culture>.tsv`, each line a key the code names
//! (`NEXT_GOAL` — English, UPPER_SNAKE), a tab, its text in that language (with
//! `\n`, `\t` and `\\` escaped). The tables are built into the program; a key missing
//! from a culture's table reads in English, one missing there as the key itself.

use std::collections::HashMap;

/// The translations shipped, by culture.
const TABLES: [(&str, &str); 12] = [
    ("de", include_str!("../../assets/i18n/de.tsv")),
    ("en", include_str!("../../assets/i18n/en.tsv")),
    ("es", include_str!("../../assets/i18n/es.tsv")),
    ("fr", include_str!("../../assets/i18n/fr.tsv")),
    ("it", include_str!("../../assets/i18n/it.tsv")),
    ("ja", include_str!("../../assets/i18n/ja.tsv")),
    ("ko", include_str!("../../assets/i18n/ko.tsv")),
    ("pl", include_str!("../../assets/i18n/pl.tsv")),
    ("pt-BR", include_str!("../../assets/i18n/pt-BR.tsv")),
    ("ru", include_str!("../../assets/i18n/ru.tsv")),
    ("tr", include_str!("../../assets/i18n/tr.tsv")),
    ("zh-Hans", include_str!("../../assets/i18n/zh-Hans.tsv")),
];

/// Key → this language's text.
#[derive(Default)]
pub struct Table(HashMap<&'static str, &'static str>);

impl Table {
    pub fn load(culture: &str) -> Table {
        let have = |c: &str| TABLES.iter().any(|(t, _)| *t == c);
        let mut map = HashMap::new();
        // English under the culture's own: what it lacks reads in English.
        for c in ["en".to_string()].into_iter().chain(super::culture::best(culture, have)) {
            if let Some((_, text)) = TABLES.iter().find(|(t, _)| *t == c) {
                map.extend(parse(text));
            }
        }
        Table(map)
    }

    pub fn get(&self, key: &str) -> Option<&'static str> {
        self.0.get(key).copied()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The lines of a table. An escaped line is unescaped once, for good (leaked): the
/// tables live as long as the program.
pub fn parse(text: &'static str) -> impl Iterator<Item = (&'static str, &'static str)> {
    text.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).filter_map(|l| {
        let (k, v) = l.split_once('\t')?;
        Some((unescape(k), unescape(v)))
    })
}

fn unescape(s: &'static str) -> &'static str {
    if !s.contains('\\') {
        return s;
    }
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(o) => out.push(o),
            None => {}
        }
    }
    Box::leak(out.into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    /// A text's placeholders: a value's name, a game text's whole reference.
    fn names(s: &str) -> Vec<String> {
        let mut n: Vec<String> = s
            .split('{')
            .skip(1)
            .filter_map(|p| p.split_once('}'))
            .map(|(a, _)| if a.starts_with("g:") { a.to_string() } else { a.split(':').next().unwrap().to_string() })
            .collect();
        n.sort();
        n
    }

    fn english() -> HashMap<&'static str, &'static str> {
        TABLES.iter().filter(|(c, _)| *c == "en").flat_map(|(_, t)| parse(t)).collect()
    }

    #[test]
    fn keys_are_english_upper_snake() {
        for (culture, text) in TABLES {
            for (k, _) in parse(text) {
                assert!(
                    !k.is_empty() && k.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
                    "{culture}: key {k:?} is not UPPER_SNAKE"
                );
            }
        }
    }

    #[test]
    fn no_key_is_in_a_table_twice() {
        for (culture, text) in TABLES {
            let mut seen = HashSet::new();
            for (k, _) in parse(text) {
                assert!(seen.insert(k), "{culture}: {k} is in the table twice");
            }
        }
    }

    #[test]
    fn every_translation_keeps_the_english_placeholders() {
        let en = english();
        for (culture, text) in TABLES {
            for (k, v) in parse(text) {
                let Some(e) = en.get(k) else { panic!("{culture}: {k} is not in the English table") };
                assert_eq!(names(e), names(v), "{culture}: {k} → {v:?} — the same placeholders as {e:?}");
                assert!(!v.trim().is_empty(), "{culture}: {k} has no text");
            }
        }
    }

    /// The keys the code names in `tr!` / `trf!`.
    fn wrapped(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        for tag in ["tr!(", "trf!("] {
            for (i, _) in src.match_indices(tag) {
                if src[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                    continue; // include_str!( and the like
                }
                let rest = src[i + tag.len()..].trim_start();
                let Some(body) = rest.strip_prefix('"') else { continue };
                out.push(body.split('"').next().unwrap_or_default().to_string());
            }
        }
        out
    }

    #[test]
    fn every_key_the_code_names_is_in_english_and_korean() {
        fn walk(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, files);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    files.push(p);
                }
            }
        }
        let mut files = Vec::new();
        walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
        let own = |c: &str| -> HashSet<&'static str> { TABLES.iter().filter(|(t, _)| *t == c).flat_map(|(_, t)| parse(t)).map(|(k, _)| k).collect() };
        let (en, ko) = (own("en"), own("ko"));
        let mut missing = Vec::new();
        for f in files.iter().filter(|f| !f.components().any(|c| c.as_os_str() == "i18n")) {
            for k in wrapped(&std::fs::read_to_string(f).unwrap()) {
                if !en.contains(k.as_str()) || !ko.contains(k.as_str()) {
                    missing.push(format!("{}: {k}", f.display()));
                }
            }
        }
        assert!(missing.is_empty(), "keys without an English or Korean line:\n{}", missing.join("\n"));
    }

    #[test]
    fn every_table_has_every_english_key() {
        let en: HashSet<&str> = english().into_keys().collect();
        for (c, text) in TABLES {
            let own: HashSet<&str> = parse(text).map(|(k, _)| k).collect();
            let missing: Vec<&&str> = en.difference(&own).collect();
            assert!(missing.is_empty(), "{c}: {} keys only in English, e.g. {:?}", missing.len(), missing.first());
        }
    }

    #[test]
    fn a_language_without_a_table_reads_english_and_an_unknown_key_itself() {
        let en = Table::load("en");
        assert_eq!(Table::load("pt").len(), en.len());
        assert_eq!(Table::load("ko").get("NEXT_GOAL"), Some("다음 목표 ▶"));
        assert_eq!(en.get("NEXT_GOAL"), Some("Next goal ▶"));
        assert_eq!(en.get("NO_SUCH_KEY"), None);
    }
}
