# Third-party

## dungeons2-mod

Most of this code base was copied from the author's own Minecraft Dungeons II mod
(`myso-kr/minecraft-dungeons-2-mod`, MIT) and adapted. That includes the name-pool
reader, attribute sessions, hold/restore, the panel and the CLI.

## Attribute names

The attribute names in `src/cheats.rs`, such as `EnduranceCap` and `LymbicEnergy`,
are the game's own reflection names. They were first read from community Cheat
Engine tables posted on FearLess Revolution (topic 35282, tables by matthew80,
Sianz and VampTY). No code or offsets from those tables are used: the tool finds
every attribute by name at run time.

## resvg

The minimap's SVG icons are rasterised with resvg and usvg (Apache-2.0 OR MIT,
https://github.com/linebender/resvg) and tiny-skia (BSD-3-Clause,
https://github.com/linebender/tiny-skia), linked into the binary from crates.io.

## egui_taffy, taffy

- egui_taffy 0.14 — https://github.com/PPakalns/egui_taffy — MIT
- taffy 0.9 — https://github.com/DioxusLabs/taffy — MIT

The panel's Flexbox and Grid layout. Linked as crates; nothing vendored.

## serde_json

`serde_json` (MIT OR Apache-2.0) reads the survey's JSON (`src/survey.rs`).

## CUE4Parse (tools/survey)

`tools/survey` uses CUE4Parse 1.2.2 (Apache-2.0, https://github.com/FabianFG/CUE4Parse) from
NuGet to read the game's cooked assets. It is a separate tool, not linked into
`hiumod`. It decompresses with Oodle (`oo2core_9_win64.dll`), which is downloaded to
the player's own `Mods\tools` and never redistributed.
