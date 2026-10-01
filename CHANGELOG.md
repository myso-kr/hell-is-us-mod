# Changelog

Versions follow [semantic versioning](https://semver.org/).

Every release records the Steam build it was verified against. Steam's `buildid`
is in `steamapps\appmanifest_1620730.acf`, and `hiumod doctor` prints it. See
[`docs/ANCHORS.md`](docs/ANCHORS.md).

## Unreleased — 0.1.0

Attaches to Steam build `24045435`, where `doctor` passes every check. No cheat has
been verified in play yet.

- Ported from dungeons2-mod: the F8 panel, the CLI, attributes by name, and holds
  whose originals are kept on disk.
- No per-build offsets. The name pool and `GEngine` are found on each attach. The
  FField layout is chosen from known candidates. The path to the player is learned
  from the engine's reflection by property name.
- A hero gate replaces the solo gate. Writes go only to the hero's attributes.
  Holds pause while the gate is closed.
- 15 cheats in four tabs (생존, 전투, 이동, 성장), all unverified. The ranges come
  from the defaults read live. Coefficients that default to 0 are added bonuses, and
  those that default to 1 are multipliers.
- `hiumod pose` prints the hero's position and camera yaw. This is the minimap's input,
  and the readings were seen moving in play.
- First play test: `god` and `stamina` work. Five coefficient attributes held in
  memory but changed nothing, so they were dropped. Speed is now the movement
  component's `MaxWalkSpeed`. Attack and dodge speed became `hero_time`, the hero's
  own time dilation. Plain fields like these go through the same checked, restorable
  path as attributes.
- Second play test: `speed` and `hero_time` work. Six more coefficient cheats did
  nothing and were dropped. That leaves 7 cheats in three tabs, 4 of them verified.
  Damage, defense, cooldowns and XP need GameplayEffects applied in game.
