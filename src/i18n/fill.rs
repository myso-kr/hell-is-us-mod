//! `format!` at run time, for a translated template: `{name}` and `{name:spec}` take
//! the value of that name; the spec is `format!`'s fill, alignment, width and
//! precision (`{d:>6.1}`). `{g:namespace/key}` is one of the game's texts in its
//! language (a place's or a person's name). `{{` and `}}` are braces.

use std::fmt::{Display, Write};

pub fn fill(template: &str, values: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let c = rest.as_bytes()[i];
        if rest[i + 1..].starts_with(c as char) {
            out.push(c as char);
            rest = &rest[i + 2..];
            continue;
        }
        if c == b'}' {
            out.push('}');
            rest = &rest[i + 1..];
            continue;
        }
        let Some(end) = rest[i..].find('}') else {
            out.push_str(&rest[i..]);
            return out;
        };
        let inner = &rest[i + 1..i + end];
        let (name, spec) = inner.split_once(':').unwrap_or((inner, ""));
        if name == "g" {
            // A name in the game's own text: {g:namespace/key}.
            out.push_str(&crate::i18n::game_text(spec));
            rest = &rest[i + end + 1..];
            continue;
        }
        match values.iter().find(|(n, _)| *n == name) {
            Some((_, v)) => put(&mut out, *v, spec),
            None => out.push_str(&rest[i..=i + end]),
        }
        rest = &rest[i + end + 1..];
    }
    out.push_str(rest);
    out
}

/// One value by `spec`: `[[fill]align][width][.precision]`.
fn put(out: &mut String, v: &dyn Display, spec: &str) {
    let mut s = spec;
    let mut fill = ' ';
    let mut align = None;
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(f), Some(a @ ('<' | '>' | '^'))) => {
            fill = f;
            align = Some(a);
            s = &s[f.len_utf8() + 1..];
        }
        (Some(a @ ('<' | '>' | '^')), _) => {
            align = Some(a);
            s = &s[1..];
        }
        _ => {}
    }
    let (width, precision) = match s.split_once('.') {
        Some((w, p)) => (w, p.parse::<usize>().ok()),
        None => (s, None),
    };
    let width = width.parse::<usize>().unwrap_or(0);
    let text = match precision {
        Some(p) => format!("{v:.p$}"),
        None => v.to_string(),
    };
    let pad = width.saturating_sub(text.chars().count());
    let (left, right) = match align.unwrap_or('<') {
        '>' => (pad, 0),
        '^' => (pad / 2, pad - pad / 2),
        _ => (0, pad),
    };
    for _ in 0..left {
        out.push(fill);
    }
    let _ = write!(out, "{text}");
    for _ in 0..right {
        out.push(fill);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_names_with_specs() {
        let (n, d) = (3, 1.25f32);
        assert_eq!(fill("{n} places, {d:.1} m", &[("n", &n), ("d", &d)]), "3 places, 1.2 m");
        assert_eq!(fill("{d:>6.2}|{n:<3}|", &[("n", &n), ("d", &d)]), "  1.25|3  |");
        assert_eq!(fill("{{x}} {n}", &[("n", &n)]), "{x} 3");
        assert_eq!(fill("{missing} {n}", &[("n", &n)]), "{missing} 3");
        assert_eq!(fill("{n:0>3}", &[("n", &n)]), "003");
    }

    #[test]
    fn a_game_text_without_the_extracted_texts_reads_as_its_key() {
        // No Mods\locale in a test run: the key made readable stands in.
        assert_eq!(fill("{g:Facts_Shared/Universal_Location_Talju} truck", &[]), "Talju truck");
        assert_eq!(fill("{g:QuickChats_Names_ST/Quickchat_Secret_Diana_Female_Name}", &[]), "Diana");
    }

    #[test]
    fn the_macro_names_its_values() {
        let here = 2;
        assert_eq!(crate::trf!("이 지역 {here}곳", here = here), "이 지역 2곳");
    }
}
