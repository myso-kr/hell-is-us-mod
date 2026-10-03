//! The player's map pins: what a pin marks (24 kinds, each with its icon in
//! `assets/pins/<word>.svg`), and a pin itself — where, what kind, the note.

use super::minimap::Point;

/// What a map pin marks — the player's own note to come back to. Each kind has its
/// icon in `assets/pins/<word>.svg`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PinKind {
    LockedDoor,
    LockedChest,
    LymbicLock,
    Puzzle,
    Code,
    KeyNeeded,
    ItemLater,
    Merchant,
    Npc,
    Quest,
    Danger,
    Boss,
    Timeloop,
    Save,
    Shortcut,
    Ladder,
    DeadEnd,
    Water,
    View,
    Treasure,
    Note,
    Home,
    Question,
    #[default]
    Mark,
}

impl PinKind {
    pub const ALL: [PinKind; 24] = [
        PinKind::LockedDoor,
        PinKind::LockedChest,
        PinKind::LymbicLock,
        PinKind::Puzzle,
        PinKind::Code,
        PinKind::KeyNeeded,
        PinKind::ItemLater,
        PinKind::Merchant,
        PinKind::Npc,
        PinKind::Quest,
        PinKind::Danger,
        PinKind::Boss,
        PinKind::Timeloop,
        PinKind::Save,
        PinKind::Shortcut,
        PinKind::Ladder,
        PinKind::DeadEnd,
        PinKind::Water,
        PinKind::View,
        PinKind::Treasure,
        PinKind::Note,
        PinKind::Home,
        PinKind::Question,
        PinKind::Mark,
    ];

    /// Its word in `minimap.txt`, and its icon's file name.
    pub fn word(self) -> &'static str {
        match self {
            PinKind::LockedDoor => "locked_door",
            PinKind::LockedChest => "locked_chest",
            PinKind::LymbicLock => "lymbic_lock",
            PinKind::Puzzle => "puzzle",
            PinKind::Code => "code",
            PinKind::KeyNeeded => "key_needed",
            PinKind::ItemLater => "item_later",
            PinKind::Merchant => "merchant",
            PinKind::Npc => "npc",
            PinKind::Quest => "quest",
            PinKind::Danger => "danger",
            PinKind::Boss => "boss",
            PinKind::Timeloop => "timeloop",
            PinKind::Save => "save",
            PinKind::Shortcut => "shortcut",
            PinKind::Ladder => "ladder",
            PinKind::DeadEnd => "dead_end",
            PinKind::Water => "water",
            PinKind::View => "view",
            PinKind::Treasure => "treasure",
            PinKind::Note => "note",
            PinKind::Home => "home",
            PinKind::Question => "question",
            PinKind::Mark => "mark",
        }
    }

    /// A word from the file; the first pins' words (`locked`, `later`) still read.
    pub fn parse(w: &str) -> Option<PinKind> {
        match w {
            "locked" => Some(PinKind::LockedDoor),
            "later" => Some(PinKind::ItemLater),
            _ => PinKind::ALL.into_iter().find(|k| k.word() == w),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PinKind::LockedDoor => tr!("LOCKED_DOOR"),
            PinKind::LockedChest => tr!("LOCKED_CHEST"),
            PinKind::LymbicLock => tr!("LYMBIC_LOCK"),
            PinKind::Puzzle => tr!("PUZZLE"),
            PinKind::Code => tr!("PIN_CODE"),
            PinKind::KeyNeeded => tr!("KEY_NEEDED"),
            PinKind::ItemLater => tr!("PICK_UP_LATER"),
            PinKind::Merchant => tr!("MERCHANT"),
            PinKind::Npc => tr!("SOMEONE_TO_MEET"),
            PinKind::Quest => tr!("QUEST_CLUE"),
            PinKind::Danger => tr!("DANGER"),
            PinKind::Boss => tr!("STRONG_ENEMY"),
            PinKind::Timeloop => tr!("TIMELOOP"),
            PinKind::Save => tr!("SAVE_POINT"),
            PinKind::Shortcut => tr!("SHORTCUT"),
            PinKind::Ladder => tr!("CLIMB"),
            PinKind::DeadEnd => tr!("DEAD_END"),
            PinKind::Water => tr!("WATER_CROSSING"),
            PinKind::View => tr!("LOOK_AROUND"),
            PinKind::Treasure => tr!("TREASURE"),
            PinKind::Note => tr!("NOTE"),
            PinKind::Home => tr!("BASE"),
            PinKind::Question => tr!("UNKNOWN"),
            PinKind::Mark => tr!("MARK"),
        }
    }

    /// Its icon's colour, for the compass and lists.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            PinKind::LockedDoor => [217, 73, 61],
            PinKind::LockedChest => [192, 96, 58],
            PinKind::LymbicLock => [138, 79, 216],
            PinKind::Puzzle => [166, 91, 232],
            PinKind::Code => [106, 111, 224],
            PinKind::KeyNeeded => [217, 139, 43],
            PinKind::ItemLater => [232, 194, 58],
            PinKind::Merchant => [63, 174, 106],
            PinKind::Npc => [62, 143, 224],
            PinKind::Quest => [232, 79, 180],
            PinKind::Danger => [224, 112, 42],
            PinKind::Boss => [184, 50, 58],
            PinKind::Timeloop => [125, 95, 224],
            PinKind::Save => [232, 162, 58],
            PinKind::Shortcut => [47, 184, 176],
            PinKind::Ladder => [91, 122, 166],
            PinKind::DeadEnd => [125, 135, 150],
            PinKind::Water => [47, 134, 200],
            PinKind::View => [63, 168, 160],
            PinKind::Treasure => [212, 167, 44],
            PinKind::Note => [168, 135, 92],
            PinKind::Home => [79, 154, 98],
            PinKind::Question => [122, 138, 160],
            PinKind::Mark => [110, 205, 255],
        }
    }
}

/// A pin on the map: where, what kind, and the player's note.
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub at: Point,
    pub kind: PinKind,
    pub note: String,
}

/// Map pins guided to as goals carry ids with bit 62 set (and 63 clear: the survey's).
pub const PIN_BIT: u64 = 1 << 62;

pub fn is_pin(id: u64) -> bool {
    id & (3 << 62) == PIN_BIT
}

impl Marker {
    /// A stable id: the world and the spot, hashed — kept while the pin stays put.
    pub fn id(&self, world: &str) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in world.bytes().chain(format!("{:.0},{:.0},{:.0}", self.at[0], self.at[1], self.at[2]).bytes()) {
            h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
        (h & !(3 << 62)) | PIN_BIT
    }

    /// What the guide calls it.
    pub fn title(&self) -> String {
        if self.note.trim().is_empty() {
            trf!("PIN", pin = self.kind.label())
        } else {
            trf!("PIN", pin = self.note.trim())
        }
    }
}
