---
name: i18n-keys
description: Adds new hiumod UI strings as English UPPER_SNAKE keys to all 12 assets/i18n/*.tsv tables at once (LF, appended, placeholders checked, equal line counts verified) and checks the tables agree. Use whenever code gains a tr!/trf! key or user-facing text, when an i18n test fails, or when the user asks to add or translate panel, overlay or CLI text.
---

# UI text keys

Rules from `.spec/I18N.md` §3: code names text by key (`tr!("NEXT_GOAL")`, `trf!("NEEDED_HERE", h =
here)`); every key has a line in each of the 12 tables (en, de, es, fr, it, ja, ko, pl, pt-BR, ru,
tr, zh-Hans), compiled in by `src/i18n/text.rs`. Only `hiumod.log` stays English.

- Key: English UPPER_SNAKE from the sentence's first words, made specific when meanings collide.
- Line: `KEY<TAB>text`, LF, `\n` `\t` `\\` escaped. Lines starting `#` are comments.
- `trf!` names every value (`{count}`, `{name:>5}`) so translations can reorder; every translation
  keeps the English placeholders and `{g:namespace/key}` tokens (game proper nouns, filled from the
  game's own text: never write a region, character or keystone name out).
- Use the game's official terms per language (look them up in `<install>\Mods\locale\<culture>.tsv`,
  game data: never commit it). Counts read "Label: {count}" so any number fits; mind case endings,
  Turkish suffixes and Korean particles after a `{…}`.

## Steps

1. Write the keys in code, then a JSON map (outside the repo, e.g. `$env:TEMP\keys.json`):
   `{"NEW_KEY": {"en": "...", "ko": "...", "de": "...", ...all 12...}}` (UTF-8; real newlines are
   escaped by the script).
2. Preview, then write:
   ```
   python .claude/skills/i18n-keys/scripts/add_keys.py "$env:TEMP\keys.json" --dry-run
   python .claude/skills/i18n-keys/scripts/add_keys.py "$env:TEMP\keys.json"
   python .claude/skills/i18n-keys/scripts/add_keys.py --check
   ```
   It refuses (writes nothing) on a bad key, a key already present, a missing culture, or a
   placeholder or token that differs from the English.
3. Run the i18n tests (verify skill, or `cargo test --release --target-dir target/next i18n`):
   `every_key_the_code_names_is_in_english_and_korean`, `every_table_has_every_english_key`,
   `every_translation_keeps_the_english_placeholders`, `no_key_is_in_a_table_twice`.
4. Help text is separate: `assets/i18n/help/<culture>.txt` (12 files); `assets/missables.tsv` keys
   are translated the same way.
