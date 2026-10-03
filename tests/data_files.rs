//! The data files the mod reads, through its public API, as they are written: the
//! survey's world file with a puzzle and a vault door, its tables, the map state's
//! file, and the mod's text with a game name in it. Unit tests cover each piece;
//! this checks they fit together.

use hiumod::puzzles::{Answer, Kind};
use std::collections::HashSet;

fn temp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("hiumod-it-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_survey_world_file_gives_its_puzzles_and_vault_door() {
    let dir = temp("survey");
    std::fs::write(
        dir.join("Talju.json"),
        r#"{"world":"Talju","actors":[
            {"name":"Pad_1","class":"Office_Keypad_ActivatorInteract_BP_C","cell":"c","at":[1,2,3],"guid":"G1",
             "puzzle":{"kind":"keypad","code":"151991"}},
            {"name":"Dial_1","class":"Ruin_DialPuzzle_Interact_BP_C","cell":"c","at":[100,0,0],
             "puzzle":{"kind":"dial","dials":[{"places":6,"solution":1},{"places":6,"solution":5}]}},
            {"name":"Door_1","class":"VOFK_Talju_DialPuzzle_Interact_BP_C","cell":"c","at":[9,9,9],"vault":true}
        ]}"#,
    )
    .unwrap();
    let s = hiumod::survey::Survey::load(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(s.doors, vec![("Talju".to_string(), [9.0, 9.0, 9.0])]);
    let keypad = s.puzzles.iter().find(|p| p.kind == Kind::Keypad).unwrap();
    assert_eq!(keypad.answer, Answer::Code("151991".into()));
    assert_eq!(keypad.guid.as_deref(), Some("G1"));
    let dial = s.puzzles.iter().find(|p| p.kind == Kind::Dial).unwrap();
    match &dial.answer {
        Answer::Dials(d) => assert_eq!(d.iter().map(|x| (x.places, x.want)).collect::<Vec<_>>(), vec![(6, 1), (6, 5)]),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_tables_count_what_is_left() {
    let dir = temp("tables");
    std::fs::write(
        dir.join("spawners.json"),
        r#"{"Talju":[{"guid":"A","timeloop":null,"entities":["x"],"at":[0,0,0]}]}"#,
    )
    .unwrap();
    std::fs::write(dir.join("vaults.json"), "[]").unwrap();
    let t = hiumod::tables::Tables::load(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(t.hollows(&HashSet::new())[0].left, 1);
    assert_eq!(t.hollows(&["A".to_string()].into())[0].left, 0);
}

#[test]
fn the_map_state_file_reads_back_as_written() {
    let mut s = hiumod::minimap::MapState::default();
    s.observe("Talju_Root_WP", [0.0, 0.0, 0.0]);
    s.toggle_marker("Talju_Root_WP", [1000.0, 2000.0, 0.0]);
    let again = hiumod::minimap::MapState::parse(&s.render());
    assert_eq!(again.render(), s.render());
}

#[test]
fn the_mods_words_take_values_and_game_names() {
    // Without the extracted game texts, a game name reads as its key.
    let line = hiumod::trf!("{g:Facts_Shared/Universal_Location_Talju} 대피 트럭 · {n}곳", n = 3);
    assert_eq!(line, "Talju 대피 트럭 · 3곳");
}
