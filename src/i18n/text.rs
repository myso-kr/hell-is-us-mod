//! The mod's own words: `assets/i18n/<culture>.tsv`, each line the Korean the code is
//! written in, a tab, its translation (`\n`, `\t`, `\\` escaped). The tables are
//! built into the program; a line missing from a culture's table comes from English.

use std::collections::HashMap;

/// The translations shipped, by culture.
const TABLES: [(&str, &str); 11] = [
    ("de", include_str!("../../assets/i18n/de.tsv")),
    ("en", include_str!("../../assets/i18n/en.tsv")),
    ("es", include_str!("../../assets/i18n/es.tsv")),
    ("fr", include_str!("../../assets/i18n/fr.tsv")),
    ("it", include_str!("../../assets/i18n/it.tsv")),
    ("ja", include_str!("../../assets/i18n/ja.tsv")),
    ("pl", include_str!("../../assets/i18n/pl.tsv")),
    ("pt-BR", include_str!("../../assets/i18n/pt-BR.tsv")),
    ("ru", include_str!("../../assets/i18n/ru.tsv")),
    ("tr", include_str!("../../assets/i18n/tr.tsv")),
    ("zh-Hans", include_str!("../../assets/i18n/zh-Hans.tsv")),
];

/// Korean → this language. Empty for Korean itself.
#[derive(Default)]
pub struct Table(HashMap<&'static str, &'static str>);

impl Table {
    pub fn source() -> Table {
        Table::default()
    }

    pub fn load(culture: &str) -> Table {
        if culture == super::culture::SOURCE {
            return Table::source();
        }
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

    pub fn get(&self, korean: &str) -> Option<&'static str> {
        self.0.get(korean).copied()
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

    #[test]
    fn every_shipped_table_parses_and_keeps_the_placeholders() {
        for (culture, text) in TABLES {
            for (k, v) in parse(text) {
                let names = |s: &str| {
                    // A value's name; a game text's whole reference.
                    let mut n: Vec<String> = s
                        .split('{')
                        .skip(1)
                        .filter_map(|p| p.split_once('}'))
                        .map(|(a, _)| if a.starts_with("g:") { a.to_string() } else { a.split(':').next().unwrap().to_string() })
                        .collect();
                    n.sort();
                    n
                };
                assert_eq!(names(k), names(v), "{culture}: {k:?} → {v:?} — the same placeholders");
                assert!(!v.trim().is_empty(), "{culture}: {k:?} has no translation");
            }
        }
    }

    /// The strings the code wraps in `tr!` / `trf!`: what follows `tr!(` up to the
    /// closing quote, unescaped as Rust would.
    fn wrapped(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        for tag in ["tr!(", "trf!("] {
            for (i, _) in src.match_indices(tag) {
                if src[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                    continue; // include_str!( and the like
                }
                let rest = src[i + tag.len()..].trim_start();
                let (raw, body) = match rest.strip_prefix("r\"") {
                    Some(b) => (true, b),
                    None => match rest.strip_prefix('"') {
                        Some(b) => (false, b),
                        None => continue,
                    },
                };
                let mut s = String::new();
                let mut it = body.chars();
                while let Some(c) = it.next() {
                    match c {
                        '"' => break,
                        '\\' if !raw => match it.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(o) => s.push(o),
                            None => {}
                        },
                        _ => s.push(c),
                    }
                }
                out.push(s);
            }
        }
        out
    }

    #[test]
    fn every_wrapped_string_has_an_english_line() {
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
        let en = Table::load("en");
        let mut missing = Vec::new();
        for f in files.iter().filter(|f| !f.components().any(|c| c.as_os_str() == "i18n")) {
            for s in wrapped(&std::fs::read_to_string(f).unwrap()) {
                if en.get(&s).is_none() {
                    missing.push(format!("{}: {s:?}", f.display()));
                }
            }
        }
        assert!(missing.is_empty(), "no English for:\n{}", missing.join("\n"));
    }

    #[test]
    fn every_table_has_every_english_line_of_its_own() {
        let en: std::collections::HashSet<&str> = TABLES.iter().filter(|(c, _)| *c == "en").flat_map(|(_, t)| parse(t)).map(|(k, _)| k).collect();
        for (c, text) in TABLES {
            let own: std::collections::HashSet<&str> = parse(text).map(|(k, _)| k).collect();
            let missing: Vec<&&str> = en.difference(&own).collect();
            assert!(missing.is_empty(), "{c}: {} lines only in English, e.g. {:?}", missing.len(), missing.first());
        }
    }

    #[test]
    fn korean_is_the_source_and_others_fall_back_to_english() {
        assert!(Table::load("ko").is_empty());
        let en = Table::load("en");
        for (c, _) in TABLES {
            assert_eq!(Table::load(c).len(), en.len(), "{c}: every line of the English table");
        }
        assert_eq!(Table::load("pt").len(), en.len(), "a language without a table: English");
    }
}
