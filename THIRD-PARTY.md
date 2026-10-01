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
