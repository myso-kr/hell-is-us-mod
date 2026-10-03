//! The game's text language: `TextCulture` in its profile save
//! (`%LOCALAPPDATA%\HellIsUs\Saved\SaveGames\HellIsUsProfile.sav`), a `StrProperty`
//! the options menu writes — `ko`, `en`, `zh-Hans`, `pt-BR`…

use std::path::PathBuf;

/// The language until the game's is read: the mod's first.
pub const DEFAULT: &str = "ko";

/// The cultures the game ships text for (HellIsUs/Content/Localization/HellIsUs/).
pub const GAME: [&str; 12] = ["de", "en", "es", "fr", "it", "ja", "ko", "pl", "pt-BR", "ru", "tr", "zh-Hans"];

pub fn profile() -> Option<PathBuf> {
    let p = crate::backup::saves()?.join("HellIsUsProfile.sav");
    p.is_file().then_some(p)
}

pub fn read(path: &std::path::Path) -> Option<String> {
    parse(&std::fs::read(path).ok()?)
}

/// The first short FString after the `TextCulture` property's type name.
pub fn parse(save: &[u8]) -> Option<String> {
    let at = find(save, b"TextCulture\0")?;
    let rest = &save[at..];
    let ty = find(rest, b"StrProperty\0")?;
    let rest = &rest[ty + 12..];
    for i in 0..rest.len().min(64).saturating_sub(4) {
        let n = u32::from_le_bytes(rest[i..i + 4].try_into().ok()?) as usize;
        if !(2..=16).contains(&n) || i + 4 + n > rest.len() {
            continue;
        }
        let s = &rest[i + 4..i + 4 + n];
        if s[n - 1] == 0 && s[..n - 1].iter().all(|b| b.is_ascii_alphabetic() || *b == b'-') {
            return Some(String::from_utf8_lossy(&s[..n - 1]).into_owned());
        }
    }
    None
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// The file a culture's table is in, among `have`: the culture itself, else its
/// language without the region (`pt-BR` → `pt`), else none.
pub fn best<'a>(culture: &str, have: impl Fn(&str) -> bool + 'a) -> Option<String> {
    if have(culture) {
        return Some(culture.to_string());
    }
    let base = culture.split('-').next()?;
    have(base).then(|| base.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_text_culture_from_the_profile() {
        let mut save = b"....TextCulture\0\x0c\0\0\0StrProperty\0\0\0\0\0\x07\0\0\0\0\x03\0\0\0ko\0\r\0\0\0AudioCulture\0".to_vec();
        assert_eq!(parse(&save).as_deref(), Some("ko"));
        save = b"TextCulture\0\x0c\0\0\0StrProperty\0\0\0\0\0\x0c\0\0\0\0\x08\0\0\0zh-Hans\0".to_vec();
        assert_eq!(parse(&save).as_deref(), Some("zh-Hans"));
        assert_eq!(parse(b"AudioCulture\0"), None);
    }

    #[test]
    fn a_culture_falls_back_to_its_language() {
        assert_eq!(best("pt-BR", |c| c == "pt").as_deref(), Some("pt"));
        assert_eq!(best("ko", |c| c == "ko").as_deref(), Some("ko"));
        assert_eq!(best("xx", |_| false), None);
    }
}
