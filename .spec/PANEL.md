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
- **Sidebar:** game status, hero gate, game data status; **Now** on its own at the top (no group);
  then the tool tabs, then the cheat groups — each
  group folds under its heading (▾/▸). Tools start open; cheats start folded unless the panel opens on
  a cheat page. The cheats heading counts the cheats on. The console is described in §6.
- **Tool tabs** (2026-10-03, at most four cards a page): **Now** (previously, before you go on, this
  region, other regions — FEATURES.md §6) · **Map** (map, pins, keys) · **Guide** (compass and guide,
  places) · **Quests** (journal, missable deeds, hand-overs, secrets) · **Clues** (the followed quest's
  Datapad entries, a search over what is known) · **Puzzles** (nearby, Lymbic locks,
  vaults, the full list) · **Collect** (collection, enemy groups, NPC stories, achievements) · **Saves** ·
  **Debug**. The "now" page is where the panel opens after a break of 30 minutes or more.
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
kept inside its monitor's work area (`hotkey.rs` `keep_on_screen`). The console and its header toggle
show only over a game menu (the game or the console in front, the cursor showing or the game paused).

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

- **Hotkey:** `VK_OEM_3` (`` ` `` ~), polled, and only honored while the game or the panel has focus. F8 is
  now selectable as the minimap key.
- **Console** (`src/ui/console.rs`):
  - A typed command runs **the same executable with those arguments**, without a window
    (`CREATE_NO_WINDOW`), and its stdout/stderr stream in line by line (`[hiumod]` log lines use the
    normal output color).
  - Built-in commands: `help` (CLI usage) and `clear`. A leading `hiumod` is stripped; a bare `hiumod`
    (which would start a second panel) is refused.
  - Up/Down recall earlier commands, Esc stops the running command, and closing the panel stops it too.
  - When the panel opens, the cursor is in the console input. A typed `` ` `` is removed (it is the panel key).
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
