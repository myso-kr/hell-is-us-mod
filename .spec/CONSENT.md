# Consent: what the mod may show and change

Added 2026-10-04 at the user's request: a Settings page where the player agrees, one thing at a time,
to what the mod shows and changes, so the game's design ("no map, no markers, find it out yourself")
is kept as far as the player wants it kept. Code: `src/infra/settings.rs` `Consent`,
`src/ui/panel/consent.rs`, the gates listed below.

## The four consents

Each is asked as a question in the player's words, built on what players run into (JOURNEY.md §1):
the question, the context and what turns on, what it costs the game, and a "yes" switch.

| Bit | Question | Turns on | Off |
|---|---|---|---|
| `MAP` | Do you get lost often? | minimap, big map, compass, route, trail, pins, quest tracker; the Map and Guide pages | no overlay at all; Map and Guide greyed |
| `PLACES` | Does what you missed bother you? | items, puzzles, people, doors on the maps; the places to go; where collectibles left and people with more to tell are | the maps keep the land, the enemies and the player's pins; Collect shows counts; Clues has no "more to tell" card |
| `ANSWERS` | When you are stuck, would you like the answer? | the Puzzles page (answers, vault codes, Lymbic locks); missable deadlines; keystone order | Puzzles greyed; the Quests page has no deadlines card |
| `CHEATS` | Are the fights or the grind wearing you down? | the cheat groups; kept cheats turned back on | cheat groups greyed; taking it back restores every cheat on |

Answers still stay covered until asked, whatever the consent.

## Rules

- **Nothing until chosen.** `settings.txt` has no `consent` line on a new install, and on one from
  before this: the mod then adds nothing (no overlay, no cheats resumed) and the panel opens on
  Settings, which says so. "Yes to all" and "No help, thanks" answer in one press.
- **Pages stay in sight.** A page not agreed to is greyed in the sidebar and in Help's page list, not
  hidden, with "allow it on the Settings page" on hover; it does not open. A page left open when its
  consent is taken back falls back to Settings.
- **The Now page shows whatever the consent**; only its tiles that would open a page not agreed to do
  nothing.
- The overlay reads the consent from `Shared::consent` (bits, 0 until chosen): without `MAP` it has no
  window; without `PLACES` it keeps only enemies of the things and no goals (pins stay).
- Saved as `consent map places answers cheats` (the granted names; a bare `consent` is "chosen, none").
