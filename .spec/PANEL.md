# Panel: window, layout, look

Decisions and history for the eframe/egui panel's layout, styling and console. The map overlay is in
`MAP.md`; UI text and translation are in `I18N.md`.

## 1. Current structure (summary)

- **Layout:** `src/ui/tw.rs` wraps egui_taffy (CSS Flexbox/Grid, see §7.4) under Tailwind-style names.
  - A column (`col`) is a one-track grid (`minmax(0,1fr)`), so row heights come from content only.
  - A row (`row`/`wrap`) is a flex row; a text block inside it (`block`) grows to fill the remaining width.
- **Cards:** masonry layout (§2). The column count depends on the number of cards and the monitor width.
  The window sizes to its content, up to 85 % of the monitor height (`MAX_SHARE` in
  `src/ui/panel/mod.rs`); beyond that the body scrolls.
- **Look:** palette, corner radii and egui visuals live in one place, `src/ui/theme.rs` (§3). Spacing uses
  four values on a 4 px scale (§4). The website (`docs/`) uses the same palette.
- **Sidebar:** game status, hero gate, game data status; **Now** and **Help** on their own at the top
  (no group); then the pages in three groups in the order they are used (2026-10-04): **Play**
  (Quests, Clues, Puzzles, Collect), **Finding the way** (Guide, Map), **System** (Saves, Debug); then
  the cheat groups. Each group folds under its heading (▾/▸). The page groups start open; cheats start
  folded unless the panel opens on a cheat page. The cheats heading counts the cheats on. The console
  is described in §6.
- **Pages, one question each** (2026-10-04): **Now** (previously, before you go on, this region, other
  regions — FEATURES.md §6) · **Quests**, what am I doing (journal, missable deeds, hand-overs, good
  deeds · mysteries · timeloops) · **Clues**, what do I know (the followed quest's Datapad entries, a
  search over what is known, NPCs with more to tell) · **Puzzles**, how do I open it (this region's
  puzzles in one card, those within 40 m first with the answer read live and then the rest from the
  survey, the two lists matched by kind within 3 m; Lymbic locks; vaults) · **Collect**, how far
  along am I (collection, enemy groups, the shard budget, achievements) · **Guide**, where do I go
  (compass and guide, places, pins and trail) · **Map**, how does it look (legend, minimap, terrain,
  big map, layer opacity, keys) · **Saves** · **Debug**. No number shows on two pages: the good deeds,
  mysteries and timeloops ring left the Collect page for the Quests page's. The "now" page is where
  the panel opens after a break of 30 minutes or more.
- **How to check:** click through every tab and capture the window (RUNBOOK.md, "Checking the panel").

## 2. Card layout: masonry and SVG icons (2026-10-03)

- **Masonry** (`tw::masonry`) replaced the fixed two-column grid. Each card's height from the previous
  frame is kept in egui memory, and cards are placed in order into whichever column is currently
  shortest, so column heights stay even. A card seen for the first time is assumed to be 240 px tall.
- **Column count** is dynamic (`Panel::fit_columns`): three columns if the page has five or more cards,
  otherwise two, limited to what fits in 90 % of the monitor width. The window width follows
  columns × card width (`tw::CARD` = 372). On a wide monitor the Collect tab (six cards) gets three
  columns.
- **SVG icons** (`src/ui/svg.rs`): rasterized with resvg into egui textures, once per (document, size),
  and cached in context memory. Used for:
  - the Map tab's "show on map" kinds and details, the pin kind picker and the pin list;
  - Guide goal steps (quest, secret, clue map to pin icons);
  - journal entries (main, good deed, mystery, time loop);
  - needed items, things to hand over, NPCs with more to tell (NPC or item icon);
  - collectible categories; puzzle, vault and enemy group rows;
  - vault symbols. This was the first use of the mechanism (`FEATURES.md` §5): vault codes in the vault
    notebook, and the vault door dial answers in "Puzzles Nearby" and the puzzle list, are drawn as
    symbols, with the name on hover.

## 3. Theme (2026-10-03)

All colors and egui visuals are defined in `src/ui/theme.rs`:

| Role | Color |
|---|---|
| Background | `#0E1217` |
| Surface | `#13181F` |
| Card | `#1A2028` |
| Border | `#2A323D` |
| Text | `#D9E1EA` |
| Dim text | `#8B96A3` |
| Accent (Lymbic blue) | `#5A9CE6` |
| Deep accent | `#22344D` |
| Status OK | `#8FD17A` |
| Status waiting | `#E8C06A` |
| Status error | `#F08278` |

- Colors previously scattered across cards, sidebar, chips, console, toggles and titles now all come
  from the theme. The website uses the same palette.
- Rule: the accent marks only what is selected, switched on, or live. The three status colors are used
  only for status.

## 4. Spacing and text size (2026-10-03)

- Four values on a 4 px scale (`src/ui/theme.rs`), replacing the scattered 4, 5, 6, 7, 8, 10 and 14:
  - `TIGHT` 4: icon to text.
  - `INLINE` 8: controls on one line; rows inside a card.
  - `BLOCK` 12: between cards, between groups, window margin.
  - `PAD` 16: card padding.
- **Cards:** 16 padding on all sides, 8 between rows, the accent bar matches the title height. Card
  width 372.
- **Sidebar:** 8 between status lines; menu items 28 px tall with 2 between them; 12 between the
  Cheats and Tools groups. Title bar 30 px; margin above and below the divider.
- **Status group** (game, hero gate, game data) uses two sizes only: the heading is small and dim, the
  status value is body size, and the extra detail (build and PID, what to do next) is small. The
  connected status is split so only the first line ("● Connected") is body size and the
  "v… · PID …" line is small.
- **Second pass the same day**, fixing what showed up when each tab was clicked and captured:
  - The window background was egui's default grey: a `set_visuals(dark)` call after `theme::install`
    overwrote the theme. Removed.
  - Row spacing inside cards was uneven: `tw::col` was a flex column, so leftover height was shared
    among growing `block`s. Columns are now one-track grids (`minmax(0,1fr)`), so row heights follow
    content; `block`s inside a row still grow to fill the width.
  - Form label column changed from 1/3 (max 150) to 42 % (max 200), because labels were wrapping too
    early.
  - Multi-line items (missable good deeds, achievements, enemy groups) group heading and description
    with `tw::item` (gap 2), so the card's 8 px gap applies only between items.
  - The Guide tab's "Places" list was cut off: a ScrollArea inside a block is stuck at the previous
    frame's height. Fixed with `min_scrolled_height`.

## 5. What can be pressed: four roles, four looks (2026-10-03)

Dashboard parts (2026-10-03, the Now page first; references: Linear, Vercel, "one hero metric and
4–6 KPIs"): `tw::hero` (a full-width panel above the cards), `tw::stat` (a KPI tile: icon and label
over a big number, pressable), `tw::grid(n, gap)` (equal columns so tiles line up), `tw::bars` (a
horizontal bar chart, two parts to one scale, with a legend), `tw::ring` (a progress ring: the count
in the middle, OK once complete), `tw::meter` (a thin progress bar). Charts are drawn with egui's
painter. Heroes: Now (live map, region, KPI tiles), Collect (collectibles, enemy groups, achievements,
secrets as rings), Puzzles (this region's puzzles, Lymbic locks, vaults as rings). Every meter and ring
shows progress (done of all), so the enemy groups card reads "beaten", not "left".

Long lists can be **grouped** (2026-10-04, `tw::order` and `tw::group_heading`): the places by tier,
the achievements by kind (from their Steam API names: story, deeds and mysteries, combat, gear,
research and collections). The Map page has a **Legend** card (`raster::legend`): the lines and
areas drawn with the settings as they are, in the colours they are drawn with.

The panel runs **once** (a named mutex; a second start shows the running one and exits) and has a
**tray icon** (`src/ui/tray.rs`: click shows or hides, right-click menu, Quit closes as × does). It is
kept inside its monitor's work area (`hotkey.rs` `keep_on_screen`). The console opens and closes with
Ctrl+` (§6) and shows while the game or this program has the keyboard.

**Each window is handled by its own handle** (2026-10-04, `hotkey.rs` `watch`, every 30 ms), not in
the panel's frames: egui runs a frame only when something happens in one of its windows, so a
decision made there waited for the next event, which could be clicks away (the console came up only
after clicking back and forth between the game and the panel).
- The console's window is kept by egui while it is open, created hidden; `watch` shows or hides it
  with `ShowWindow` the moment the condition changes, and asks the panel for a frame.
- The panel minimised (its taskbar button) is hidden as its own — does: the console with it, and the
  keyboard given back to the game (`take_focus`, as Windows has already given it to some other
  window and would refuse a plain request). Left minimised, the panel still counted as showing.
  Shown again, it is restored first.

**The shell is one grid of three rows** (`tw` `sidebar` + `span_all`): the header, the body (the
sidebar | the page), the footer across both columns. The footer is two even halves (`tw::share`:
`flex-1 basis-0 min-w-0`), so what they hold never widens the row or the window; each is two rows,
mirrored: buttons first (restore all and "Keep settings" | the update), small text second (a note |
the version, GitHub, the copyright). The scroll bar's lane is the frame's right margin rather than a
gutter of its own: the frame gives it up on the right and the header and footer take it back as
padding, so every edge is `BLOCK` from the window's whether the page scrolls or not.

A ScrollArea is never taller than the room its parent has, and a taffy leaf's room is the height it
reported the frame before. Starting from 0, the page's scroll area stayed 0 tall and the page drew
nothing (it worked only while the footer sat in the same leaf and gave it height). The page's room
is given outright (`allocate_ui_with_layout` at `page_height`); it still shrinks to its content.

**The panel's size** (2026-10-04): most players are on 1920×1080 (48 %) or 2560×1440 (27 %) —
Steam's hardware survey, September 2026. The window is at most 55 % of the monitor's width (as many
columns of cards as fit: two on 1080p, three on 1440p and wider) and 72 % of its height (was 90 % and
85 %, which on 1080p hid most of the game); the page scrolls inside.

**The Now page** (2026-10-04, `panel/now.rs`), after the pains players name most (JOURNEY.md §1):
the hero (the live map, the region, the KPI tiles), then a grid of the page's columns (`tw::span`,
cards in a row stretched to one height, their rows kept at the top): **the story followed** (two
columns: where the last session ended when it was 30 min or more ago, the quest and its progress, the
guide's next goal, what was last learned on each lead), **what can be done here now** (hand-overs by
whom and how far, the Lymbic locks the rods held open, the nearest places), **side stories under way**
(good deeds, mysteries and timeloops begun, this region's first), **worth a trip** (the three regions
with the most to do now and what: hand-overs, locks, what is left; the rest in a line), **before you go
on** (missable deadlines, the keystone order). The old "previously" card became the story card's
first line.

**Cards spanning columns** (`tw::spans`, 2026-10-04): Quests (the journal 2 wide, hand-overs; the
missable deeds 2 wide, the side-story rings), Clues (the quest's clues 2 wide, the search; the NPCs with
more to tell 3 wide), Puzzles (this region's puzzles 2 wide, the locks; the vaults 3 wide), Collect
(collection, enemy groups, shard budget; the achievements 3 wide), on three columns, the cards of a row
one height. Masonry stays for the Map and Guide pages and the cheats, whose cards are alike in width.

**Long lists scroll inside their card** (`tw::scroll_list`): the achievements grow to 420 px, then
scroll, over a taffy column of their own, given its room outright (the 0-height trap above).

**The backdrop** (`panel/backdrop.rs`): faint contour lines drifting over two hills and, every 14 s,
a dashed route finding its way across them with the goal's diamond glowing at its head, then fading,
after the introduction video. Drawn first in the panel's frame, so it shows between the cards and
behind the sidebar; 15 frames a second; a "the panel's background moves" switch on the Settings page
(`motion` in settings.txt). Measured: the panel's process uses the same CPU with it on or off (about
66 % of a core while the big map draws in game; the backdrop is a rounding error).

**Cards are thin glass** (`tw` `background`): the card colour at about 80 % opacity, so the backdrop
shows faintly through; a white sheen at 4 % fading out by 40 % of the height; a hairline of light
along the top edge inside the corners; the edge a little softer. No blur: egui has none, and the
translucency over a dark, quiet backdrop reads as glass without it.

**The splash** (`panel/splash.rs`) is a window of its own in the middle of the screen, drawn after
the introduction video's closing card and the site's hero: the ground colour, contour lines
drifting, the compass strip with its glowing goal, the double diamond, the name, the loading steps
(game, hero gate, game data) and a bar. The panel opens off the screen and lays itself out there
(its frames run as a shown window's do; the hotkey thread leaves it be while `Shared::splash`); the
splash goes, and the panel comes to its place, once the splash has shown 1.8 s, the first reading
is in with nothing still being read, the cheats kept from last time are back on, and the panel has
kept its size for 0.3 s, or after 8 s whatever is left.

**The footer** holds restore all, "Keep settings" (the cheats on now come back next launch), the
version, updates, GitHub and the copyright. Updates are checked once at start. **Updates** (`src/ui/update.rs`) are by hand at each step: check (GitHub's latest release
through Windows' `curl.exe`), download (the zip and its `.sha256`, checked, unpacked with
`tar.exe`), restart. Restarting renames the running files aside (`*.update-old`, removed at the next
start), copies the new ones in, starts the new panel with `HIUMOD_AFTER` = this pid (it waits for
this one to exit before taking the one-panel mutex), and closes as × does.

The Map page's **previews** (2026-10-03, split 2026-10-04): while that page shows
(`Shared::preview_wanted`), the overlay draws both maps with the settings as they are, four times a
second, whatever the display mode: the minimap at the top of its card (`Shared::preview`), the big
map at the top of its card in the game window's shape (`Shared::preview_big`, drawn straight at
640 px wide rather than at the screen's size and made small).

Scrolling shows itself (2026-10-03): solid scroll bars whenever there is more (theme.rs; egui's
default bars float, thin, only on hover) and `tw::scroll` fades the edge that hides content into the
colour behind it. "Elsewhere" counts are `tw::regions`: a dim label and quiet chips, most first.
Internal names stay off the cards: puzzles read kind, answer shape and distance (no actor class),
timeloops their letter, or the place they are named after when it is not the region.

States are shown as **chips** (`tw::chip`/`tw::pill`, a tone's text on a faint wash of it: OK done or
fine, WAIT waiting, BAD about to be lost, ACCENT chosen or known, Quiet a plain label); keys as
**keycaps** (`tw::keycap`). Cards have no accent rail any more: the accent marks what is chosen or
live, so a card's header is its title over a hairline.

Every control belongs to one role, and each role has one look, so a card never makes the player
work out which of two alike buttons does what.

| Role | Look | Where |
|---|---|---|
| Go there | The line itself, pressable (`tw::line`, `tw::pick_with`); the line guided to stays marked. No Guide buttons. | Lymbic locks and rods, puzzles nearby, the puzzle list, vaults, enemy groups, quests, places |
| Reveal | One small button at the end of the line (`reveal` in `deep.rs`): Show answer / Show code ↔ Hide. The answer appears under the line. | Puzzles, vaults, hidden achievements |
| Act | A normal-size button | Apply, Save, Back up now, Clear trail, Got it, Open page, Back to auto, Undo |
| On/off | A switch (`toggle`), as tall as a line of body text so a top-aligned label sits on its first line. A choice among values is a segmented row. | Settings, list filters, keeping cheats on |

- Explanations go in hover text, not on the card: a card shows the state (numbers, a status chip),
  the why is one hover away. UI text avoids the em-dash; a colon, parentheses or two sentences read
  better in every language the panel ships (2026-10-03).
- A line carries at most one button. A line with nowhere to go (a puzzle solved, a vault opened, a rod
  held) is drawn by the same `tw::line` as a disabled selectable: the same padding and height, so text
  and spacing match the pressable lines (a frameless button has no padding and would not).
- Exceptions: a map pin's row holds a text field, so it keeps a "Guide" selectable and a × at its end;
  folding (the sidebar groups, the map's details) is a frameless ▾/▸ line.

## 6. Panel key `` ` `` (~) and CLI console (2026-10-03)

Requested by the user: move the panel hotkey to `` ` `` (~) and add a console overlay for CLI commands that
is ready for input whenever the panel is open.

- **Hotkey:** `VK_OEM_3` (`` ` `` ~), polled, and only honored while the game or the panel has focus.
- **The console's key** (2026-10-04): **Ctrl+`**, fixed. It first had a toggle in the panel's header
  that showed only over a game menu, which was hard to reach; a key of its own (F8, settable) was
  tried and dropped: the console is for checking things, not for play, so it shares the panel's
  key with a modifier. Shift was first; it is the game's sprint (and Shift+click its charge attack),
  so a ` pressed while running opened the console: Ctrl, which the game's default controls do not
  use, since 2026-10-04. It toggles `Shared::console_open` in the hotkey thread, shows a hidden panel (the
  console is drawn by it) and puts the cursor in the input; the header keeps the key as a hint. The
  console shows while it is open and the game or this program has the keyboard.
- **The console without the panel** (2026-10-04): the console is an egui viewport of the panel's
  window, drawn only while its frames run, and eframe runs none for a hidden window. So with the
  console open the panel is not hidden but parked off the screen (`hotkey::park`, `Shared::parked`):
  Ctrl+` opens the console alone, the panel staying out of sight, and ` (or —) hides the panel while
  the console stays. Closed, the panel goes back to hidden. The backdrop does not move while parked.
- **Function keys left free** (2026-10-04): the game's defaults use F1 (show the HUD) and F7 (photo
  mode), Steam's F12 takes a screenshot (Game8's and Magic Game World's control lists); the key
  pickers do not offer them (`minimap::TAKEN_KEYS`), and a saved setting on one goes back to the
  defaults.
- **Default keys F2–F5** (2026-10-04, `minimap::DEFAULT_KEYS`): F2 map display, F3 compass, F4 next
  goal, F5 pin — beside the game's F1, none of them the game's. They were F9, F6, F10, F11; a saved
  file with all four of those untouched moves to the new ones, a key the player chose stays. Shift+Tab
  for the map display was weighed and left: it is the Steam overlay's key, and taking it from the
  game would take it from the overlay too.
- **Console** (`src/ui/console.rs`):
  - A typed command runs **the same executable with those arguments**, without a window
    (`CREATE_NO_WINDOW`), and its stdout/stderr stream in line by line (`[hiumod]` log lines use the
    normal output color).
  - Built-in commands: `help` (CLI usage) and `clear`. A leading `hiumod` is stripped; a bare `hiumod`
    (which would start a second panel) is refused.
  - Up/Down recall earlier commands, Esc stops the running command, and closing the panel stops it too.
  - When the console opens, the cursor is in its input. A typed `` ` `` is removed (it is the panel key).
- **Placement:** the first version was a 220 px strip under the panel, collapsible from its title line
  ("Console ▾"). The same day, at the user's request ("a translucent UI at the very top, like
  Half-Life"), it became a **translucent window dropping down from the top of the game window**:
  - an egui immediate viewport titled "Hell Is Us Mod — console": undecorated, transparent, always on
    top, no taskbar entry;
  - game window width × 38 % of its height (minimum 180 px), background (8, 10, 14, α 200);
  - drawn only while the panel is visible. eframe does not render frames while hidden, so when `` ` `` hides or
    shows the panel, `src/ui/hotkey.rs` finds the console window by its title and hides or shows it too;
  - when open, it takes keyboard focus.

## 7. Layout history

How the panel reached §1. Each step replaced the previous one; `ui/layout.rs` and the 12-column grid no
longer exist. Kept for the reasons things changed.

### 7.1 Console-style two columns (2026-10-02)

Request: merge north correction with guide/map, make the panel wider, use a two-column "SaaS console"
style. Width went from 470 to 960 with a 172 px left sidebar, and the map and guide became one page.
Superseded by the tabbed layout (`FEATURES.md` §1). A saved tab value `guide` was read as `map`.

### 7.2 Fixed grid (2026-10-02)

Problem: inner cards overlapped. egui `columns` and `Grid` grow to fit content and do not clip, so fixed
180 px sliders, four-button rows and unwrapped labels in ~370 px columns spilled over neighbouring
cards. `ui/layout.rs` added equal-width clipped columns and fixed-width form fields (112 px labels).
Replaced because clipping hid content instead of fitting it. Changes that survived: north correction
became a dropdown, "heading up" became "Up is: North / Camera", terrain got its own card, and long goal
and place names are ellipsized with the full name on hover.

### 7.3 12-column grid, maximum height, body scroll (2026-10-03)

Feedback: content should fit the width, not be clipped; expanding sections made the window taller than
the screen. `ui/layout.rs::row` became a 12-column grid that stacked cells vertically below a minimum
width (300 px cards), with text wrapping by default. The lasting part is the height rule: window height
follows content up to 85 % of the monitor, and the body scrolls inside while the sidebar and bottom bar
stay fixed. The grid itself was replaced by taffy (§7.4) because hand-computed spans still guessed
widths.

### 7.4 CSS Flexbox/Grid via taffy; trimmed settings (2026-10-03)

Request: Tailwind-style dynamic flexbox and grid. Adopted `egui_taffy 0.14` (egui 0.36, taffy 0.9, a
W3C Flexbox/Grid implementation, MIT) and wrote `src/ui/tw.rs` (§1).

- Components: `card`; `field` (label plus a growing, wrapping control); `switch`; `choices`; `slider`
  (the track grows and the value box is a separate node, so no width guessing); `block` (grow,
  `min-w-0`, wrapping text and lists); `w` (shrink-0, nowrap widget).
- Pitfalls found from captures:
  1. A root sized `auto` is fit-content per CSS, which collapsed cards to a single 320 px column. Fixed
     with `w-full`.
  2. Measuring small widgets in wrap mode used a zero width on the first frame, so they folded one
     character per line. `w` is nowrap.
  3. A text block in a row without grow stayed at its narrow measured width. `block` grows.
- The original `cards(min)` auto-fit grid and fixed 360 px card width were later replaced by masonry
  (§2).
- **Trimmed settings** (request: cut excess elements; F9 cycling becomes default behaviour). Removed from
  the UI, saved values kept: F9 cycling, north correction, terrain color legend and structure counts,
  big map description, "show on map" description, the investigations-in-progress card, "Places" and
  key descriptions, the game menu card (its switch moved to the minimap card), region name and
  coordinates, the Cheats page's notes card. The position-saving description became one line.

**Cards that keep their place** (2026-10-05, after "the Guide page jumps about as things are followed
and let go"): `tw::masonry` put each card in the shortest column by last frame's heights, so a card
that grew or shrank moved the others between columns. `tw::masonry_pinned` takes a column per card
(`Some(column)`, the last one at most): pinned cards stay put, the rest fill around them; and the
columns of the frame before are kept while they are within 160 px of the best, so no card moves for a
small change in height. The Guide page pins the auto guide to the first column and what is followed
to the last.

**Found in the same pass over every page** (captured through UI Automation): the Follow toggle now
keeps its room on every line (`TOGGLE_W`), so the buttons before it line up whether a line can be
followed or not; the Now page's "before you go on" lines broke one letter per line (a chip beside a
row of its own: the chip is the line's icon now); a followed collectible's name began with its kind's
key (`RECORDS`); one person was listed twice among those with more to tell (two placements a few
metres apart: the nearer); the Map page's terrain view wrapped its four choices one per line (one
control now); the auto guide's "no goal in this region" spoke of the followed quest (the main story),
the next-goal key's name and the Help page's line for the Guide page were from before following.

**One width for every page** (2026-10-05, asked: "find the width that does not change from page to
page"): the columns were two or three by the page's cards, and the window grew and shrank as pages
were changed (988 and 1372 px on a 3440-wide screen). They are now the monitor's, the same on every
page: as many 360 px cards as fit in 55 % of its width, three at most (two on 1080p, 988 px; three on
1440p and wider, 1372 px). Pages with fewer cards spread them over the width (masonry fills the
columns; the Settings page's two-column grid widens); the debug page is as wide as the rest. The
height still follows the page, up to 72 % of the monitor's.

**One height too** (2026-10-05, asked next): the page's room is 72 % of the monitor's height less the
chrome and footer on every page (`set_min_height` on the room: the scroll area itself shrinks to a
short page), so the footer stays put and a long page scrolls. Captured: every page 1372 x 1008 on
this screen.

**The Map page's masonry** (same pass): its middle "column" was one masonry item holding four cards
(minimap, terrain, big map, layer opacity), three times as tall as the others. Each is a card of its
own now (`map_card(which)`), and so are the Guide page's map layers and pins (`marks_card(which)`):
the columns even out (minimap | legend, keys | big map, terrain, opacity).

**Col-span pages** (2026-10-05): the Guide and Map pages read as empty next to the Play pages
(they are mostly settings). Both moved from masonry to `tw::spans`. What is *seen* is two columns
wide, what is *done* is one: Guide = area map (2) | auto guide + followed (1, `tw::stack`),
places in two columns (2) | layers (1), journey (2) | pins (1). Map = previews and presets (3),
then minimap | big map | terrain + opacity, then legend (2) | keys (1). `tw::stack` puts several
cards in one cell: rows `auto … 1fr`, so the last card fills the rest and no gap opens between
them. The area map is drawn by the overlay (`Shared::ops`, 400 px, every 0.5 s, only while the
Guide page shows) with the routes as last worked out; the panel keeps the `View` it was drawn
with, so a pointer over the image projects back onto the places. The journey's "this run" is the
trail's length less what it was when the panel first saw each world; its list is
`MapState::done`, the follows that ended by themselves (not kept across runs).

**Nothing past a card's edge** (2026-10-05): the Terrain card's view choices ran past its right
edge. `field` gave the label a fixed share that does not shrink and the controls what was left,
and a set of choices is one block that cannot wrap within itself. The field's row wraps now and
its controls take their one-line width as their basis: when they do not fit beside the label,
they go under it, the row's width. Every page captured after: nothing else ran past a card.

**The legend, in sections**: lines and areas (four across), then the icons, each kind shown in an
inset of its own, two to a row, its sorts under its name (a hidden sort dimmed; the kinds with a
single icon share one inset), then the goals' icons. As one run of rows it read as a jumble.

**System and cheat pages, col-span** (2026-10-05), as the Guide and Map pages:

- **Saves**: an overview across the top (the last backup, backups kept, what they take, the game's
  last save, as stat tiles; Back up now, Open folder), then the backups' timeline two columns wide
  (ten shown, from six) and the game's save files beside it.
- **Debug**: a status card (memory now and at its peak, cheats on, verified, the log folder) beside
  what the cheats on write (two wide; what the columns mean is the header's "?"), then the test
  record across the page in two columns, no longer in a 260 px scroll. The egui-frame `section`
  is gone: every part is a card like the rest.
- **Cheat groups**: a group's cheats two wide, as tiles two to a row (the switch and name, the
  slider under them, the game's value now at the foot), every row's tiles as tall as each other;
  what is on beside them. Movement adds the teleport to what is followed (two wide) and the saved
  positions.
