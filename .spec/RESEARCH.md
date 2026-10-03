# Research before starting (2026-10-01)

Web research plus analysis of the local executable. Nexus, FearlessRevolution and
PCGamingWiki returned 403 to direct access, so they were read through the Nexus
GraphQL API, the Wayback Machine and the MediaWiki API. What could not be
confirmed is listed at the end.

## Engine and protection

- UE **5.5.4**: https://www.pcgamingwiki.com/wiki/Hell_is_Us
- No Denuvo:
  https://ixbt.games/en/news/2025/09/02/hell-is-us-vzlomali-do-reliza-no-nekotorye-xotiat-ee-kupit-iz-za-nebolsogo-vesa-failov.html.
  PCGW says the executable is DRM-free when run directly. Locally the PE sections
  look normal and there is no EAC folder.
- `EOSSDK-Win64-Shipping.dll` is presumably for EOS services; there is no evidence
  of anti-cheat.

## Existing mods (Nexus, all 36 checked; no cheat, trainer, map or compass mods)

| # | Name | Approach |
|---|---|---|
| 43 | RE-UE4SS (preconfigured for UE 5.5) | `dwmapi.dll` proxy; only the engine version override is needed |
| 27 | Camera and Misc. | UE4SS Lua; plain UE4SS with `[EngineVersionOverride] 5/5` |
| 3 | Simple Mod Loader | developer console + Blueprint mod loader |
| 22 | IOStore sideloader | `dsound.dll`, bypasses signature checks → loads `~mods` paks |
| 33 | Mapping File | `.usmap` for FModel |
| 4 | FreeCam and Teleport | (README not checked) |
| 16 | ReShade addon | FOV, camera, developer console |
| 26 | Auto Healing Pulse | external exe using screen recognition |

- Vortex extension: https://www.nexusmods.com/site/mods/1417. The signature bypass
  is required; UE4SS mods go in `ue4ss/Mods`, Blueprint paks in `Paks/LogicMods`.
- Universal Unreal Engine 5 Unlocker works (PCGW).

## Cheat Engine tables (FearlessRevolution)

- t=36338 (ndck1, game v1.3.36): an inventory editor. Reported not to work on 1.5+.
- t=35282: tables by matthew80, Sianz and VampTY. Health, stamina, Lymbic, drone
  cooldown, sprint speed.
  - `StoryHero_BP_C` +0x688 → ASC, +0x1088 → SpawnedAttributes, +0x330
    CharacterMovement.
  - Set order: [0] Endurance [1] Lymbic [2] Damage [3] Weapon [4] Movement
    [5] Poise [6] Defense [7] miscellaneous (XP, invincibility window, cooldowns).
  - Infinite consumables: patch `mov [rcx+48],edx` (item quantity at +0x48).
  - An early "infinite health" was reported to make enemies invincible too, a side
    effect of hooking shared code.
- The full list of attribute names came from these tables (used in
  `src/cheat/cheats.rs`).

## Minimap precedents

1. Palworld PalMiniMap #3915: UE4SS Lua alone draws the world-map texture and icons
   into the game's UMG. Lightweight, no SceneCapture.
   https://github.com/jeankassio/PalMiniMap
2. Palworld Basic MiniMap #146 / YetAnotherMinimap #2973: LogicMods Blueprint paks.
   https://github.com/SvenBrnn/pal-yet-another-minimap-mod
3. Wukong Minimap #1172: Rust + hudhook (DX12) + imgui, `dwmapi.dll` proxy
   (conflicts with UE4SS). The closest precedent for adding a minimap to a game
   that has none. https://github.com/jaskang/wukong-minimap
4. Hogwarts Legacy Dynamic Compass #1063: Blueprint loader + UMG compass.

No public example was found of an external transparent window driven by memory
reads.

## Paks and tools

- utoc flags: Compressed | Encrypted | Signed | Indexed. No AES key has been
  published; AESDumpster (https://github.com/GHFear/AESDumpster) can extract it
  locally.
- retoc (https://github.com/trumank/retoc), UnrealReZen, FModel with the #33 usmap.

## Found in the local executable

- Classes: `CharlieCharacterHero`, `CharlieHeroAbilitySystemComponent`,
  `CharliePlayerController`, `CharlieGameInstance`, `CharlieCheatManager`,
  `CharlieDebugMapCrawlerSubsystem`, `CharlieToggleDebugUI`, and cheat widgets
  (`CheatCategoryEntryWidget` and others). The development cheat UI is still in the
  shipping build.
- Map and compass: `CompassSumg`, `ShowCompass`, `CompassOptions`,
  `Close/Middle/FarWorldMapSoftTexture`, `TeleportToMarker`,
  `TeleportMarkerComponent`.
- Attribute set classes: see `ARCHITECTURE.md`.

## Not confirmed

- The Nexus documentation tabs (how #3 and #4 enable the console); the
  FearlessRevolution Patreon tables.
- Currency and XP attribute names, the AES key, whether retoc supports UE5_5.
- A public Dumper-7 SDK (not on Dumpspace). If needed, dump it directly with UE4SS.
