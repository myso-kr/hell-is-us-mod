# Consent: what the mod may show and change

Added 2026-10-04 at the user's request: a Settings page where the player agrees, one thing at a time,
to what the mod shows and changes, so the game's design ("no map, no markers, find it out yourself")
is kept as far as the player wants it kept. Code: `src/infra/settings.rs` `Consent`,
`src/ui/panel/consent.rs`, the gates listed below.

## The eight consents

Four at first; split into eight on 2026-10-05 when the guide grew (the user: the Settings questions
should cover more as features multiply). Each is asked as a question in the player's words, built on what players run into (JOURNEY.md §1):
the question, the context and what turns on, what it costs the game, and a "yes" switch.

| Bit | Question | Turns on | Off |
|---|---|---|---|
In the order a player meets them:

| Bit | Question | Turns on | Off |
|---|---|---|---|
| `MAP` (1) | Do you get lost often? | minimap, big map, compass, trail, pins; the Map page | no maps; Map greyed |
| `GUIDE` (16) | Not sure where to go next? | the auto guide and tracks: routes, rings, the compass target; the Guide page | nothing followed, no route; Guide greyed |
| `PLACES` (2) | Does what you missed bother you? | items, puzzles, people, doors on the maps; the places to go; goals that reveal a hidden thing's place | the maps keep the land, the enemies and the player's pins; such goals left out of the guide |
| `STEPS` (32) | Stuck at a locked door, not sure what comes first? | the requirement graph in the guide: gated goals step to their chain's first thing, flooded ones to the drain, a route through a barrier to what opens it (GRAPH.md §3, §9) | the guide goes straight to its goal; the Guide card says so |
| `ANSWERS` (4) | When you are stuck, would you like the answer? | the Puzzles page; goals that give an answer away (an order or position puzzle's device) | Puzzles greyed; such goals left out of the guide |
| `MISSABLES` (64) | Want a warning before something is missed for good? | the banner when the next story beat ends a good deed | no such banner |
| `HUD` (128) | Is information over the game screen all right? | the quest tracker and its context lines, the banner | no panels over the game (the maps are `MAP`'s) |
| `CHEATS` (8) | Are the fights or the grind wearing you down? | the cheat groups; kept cheats turned back on | cheat groups greyed; taking it back restores every cheat on |

### What the guide leaves out, and says so

Every goal carries `Goal::reveals` (`Nothing`, `Places`, `Answers`): what guiding there gives away
beyond the story's way. A live quest goal reveals nothing; a hidden thing's place reveals `Places`; a
graph step at an order or position puzzle's device reveals `Answers`, and other steps take what the
goal they are for reveals. The overlay keeps only the goals the consent allows (`by_consent`) and
counts the rest in `Shared::withheld`; the Guide page's auto card says how many hidden places and how
many answers were left out, and, when `STEPS` is off, that the guide goes straight past doors and
water. The worker reads the consent through `settings::LIVE` (it builds the graph's steps only with
`STEPS`).

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
- Saved as `consent v2 map guide places …` (the granted names; a bare `consent v2` is "chosen,
  none"). A line without `v2` is from the four: `map` carries on as `guide` and `hud` too, `answers`
  as `steps` and `missables`, so no one loses what they had.
