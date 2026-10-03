//! The mod in the game's language (.spec/I18N.md).
//!
//! The language is the game's own text setting — `TextCulture` in the profile save
//! (culture.rs), checked every few seconds, so changing it in the game's options
//! changes the mod with it.
//!
//! Two kinds of text:
//! - **The game's names** — items, NPCs, regions — come from the game's own
//!   translations, extracted by `doctor locale` (tools/survey `--locale`) into
//!   `Mods\locale\<culture>.tsv` and `names.tsv` (names.rs). All twelve languages
//!   the game ships.
//! - **The mod's own words** are keys in the code — English UPPER_SNAKE, in `tr!` or
//!   `trf!` (`tr!("NEXT_GOAL")`); `assets/i18n/<culture>.tsv` gives each key its
//!   text, Korean (`ko.tsv`) like every other language (text.rs). A culture without
//!   a table, or a key a table lacks, reads in English, then as the key.

pub mod culture;
pub mod fill;
pub mod names;
pub mod text;

use std::sync::{Arc, OnceLock, RwLock};

/// What one language needs: its words for the mod's text, the game's text.
pub struct Lang {
    pub culture: String,
    pub text: text::Table,
    pub names: names::Names,
}

fn current() -> &'static RwLock<Arc<Lang>> {
    static LANG: OnceLock<RwLock<Arc<Lang>>> = OnceLock::new();
    LANG.get_or_init(|| {
        RwLock::new(Arc::new(Lang {
            culture: culture::DEFAULT.into(),
            text: text::Table::load(culture::DEFAULT),
            names: names::Names::default(),
        }))
    })
}

/// The language in use.
pub fn lang() -> Arc<Lang> {
    current().read().unwrap().clone()
}

/// The culture in use: `ko`, `en`, `zh-Hans`…
pub fn culture() -> String {
    lang().culture.clone()
}

/// Switch to `culture` (loading its tables), or stay when it is the one in use.
pub fn set(culture: &str) {
    if lang().culture != culture {
        load(culture);
    }
}

/// Follow the game's setting: read it, and switch when it changed. Cheap enough to
/// call every few seconds (one file's modified time, read when it changed).
pub fn follow_game() {
    static SEEN: OnceLock<std::sync::Mutex<Option<std::time::SystemTime>>> = OnceLock::new();
    let Some(path) = culture::profile() else { return };
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let mut seen = SEEN.get_or_init(Default::default).lock().unwrap();
    if modified.is_some() && *seen == modified && !lang().names.is_empty() {
        return;
    }
    *seen = modified;
    drop(seen);
    let c = culture::read(&path).unwrap_or_else(|| culture::DEFAULT.into());
    if lang().culture != c || lang().names.is_empty() {
        load(&c);
    }
}

/// Load `culture`'s tables and use them (even when it is the one in use: its
/// files may have appeared since).
fn load(culture: &str) {
    let l = Lang { culture: culture.into(), text: text::Table::load(culture), names: names::Names::load(culture) };
    crate::logfile::line(&format!("language: {culture} ({} mod lines, {} game names)", l.text.len(), l.names.len()));
    *current().write().unwrap() = Arc::new(l);
}

/// The mod's own words in the language in use, by key (`NEXT_GOAL`); a key no table
/// has reads as itself.
pub fn tr(key: &'static str) -> &'static str {
    lang().text.get(key).unwrap_or(key)
}

/// A Datapad fact's text and story unit, by its asset name (names.rs `facts.tsv`).
pub fn fact(name: &str) -> Option<names::Fact> {
    lang().names.fact(name)
}

/// A place's name in the game's language: its `Universal_Location_<name>` text
/// (`ArcasSpire` → the Arcas Spire's name).
pub fn location(name: &str) -> Option<String> {
    lang().names.game_text("Facts_Shared", &format!("Universal_Location_{name}"))
}

/// An item's name, by its data asset's name or path (`/Game/Items/…/Key_Item_DA`).
pub fn item(asset: &str) -> Option<String> {
    lang().names.item(asset)
}

/// An NPC's name, by its blueprint class (`Convo_TaljuMechanic_BP_C`): the name the
/// hero knows them by — the real one once a fact told it.
pub fn npc(class: &str, facts: &std::collections::HashSet<String>) -> Option<String> {
    lang().names.npc(class, facts)
}

/// A region's name, by its world (`SenedraForest`).
pub fn region(world: &str) -> Option<String> {
    lang().names.region(world)
}

/// A text the game gave as its English source (a good deed's title, a place, before
/// the game has shown it once), in the language in use.
pub fn from_source(english: &str) -> Option<String> {
    lang().names.from_source(english)
}

/// `namespace/key` — one of the game's texts in the language in use; without the
/// extracted texts (`doctor locale` not run), the key made readable.
pub fn game_text(reference: &str) -> String {
    let (ns, key) = reference.split_once('/').unwrap_or(("", reference));
    lang().names.game_text(ns, key).unwrap_or_else(|| {
        let last = key.rsplit('_').find(|p| !matches!(*p, "Name" | "Real" | "Female" | "Male")).unwrap_or(key);
        last.to_string()
    })
}

/// The mod's words for a key not written in the code (a data table's): translated
/// like `tr!`, its `{g:…}` names filled.
pub fn text(key: &str) -> String {
    let l = lang();
    fill::fill(l.text.get(key).unwrap_or(key), &[])
}

/// A world's name to show: the region's, else the world's own.
pub fn place(world: &str) -> String {
    region(world).unwrap_or_else(|| world.to_string())
}

fn known() -> &'static RwLock<Arc<std::collections::HashSet<String>>> {
    static KNOWN: OnceLock<RwLock<Arc<std::collections::HashSet<String>>>> = OnceLock::new();
    KNOWN.get_or_init(Default::default)
}

/// The facts the hero knows, by name — what tells an NPC's real name (engine.rs
/// sets them as it reads them).
pub fn set_known(facts: &std::collections::HashSet<String>) {
    if **known().read().unwrap() != *facts {
        *known().write().unwrap() = Arc::new(facts.clone());
    }
}

/// A quest track's subject (`Quest01_Tania`, the part after the quest) by what the
/// hero knows now: a person's or a place's name in the game's language.
pub fn subject(unit: &str) -> Option<String> {
    let facts = known().read().unwrap().clone();
    lang().names.subject(unit, &facts)
}

/// An NPC's name by what the hero knows now (`set_known`).
pub fn npc_known(class: &str) -> Option<String> {
    let facts = known().read().unwrap().clone();
    npc(class, &facts)
}

/// `tr!("NEXT_GOAL")`: the mod's words in the game's language, by key.
#[macro_export]
macro_rules! tr {
    ($s:literal) => {
        $crate::i18n::tr($s)
    };
}

/// `trf!("NEEDED_HERE", h = here)`: `format!` for a translated text. Each value is
/// named, so a translation may put them in its own order.
#[macro_export]
macro_rules! trf {
    ($s:literal $(, $name:ident = $val:expr)* $(,)?) => {
        $crate::i18n::fill::fill($crate::i18n::tr($s), &[$((stringify!($name), &$val as &dyn ::std::fmt::Display)),*])
    };
}
