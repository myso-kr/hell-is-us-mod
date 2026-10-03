## What does this change

<!-- A line or two. If it fixes an issue, "Fixes #12". -->

## Kind

- [ ] Translation (`assets/i18n/`, `tools/site/strings/`)
- [ ] The tool (`src/`)
- [ ] The survey tool (`tools/survey/`)
- [ ] The website or the video (`tools/site/`, `tools/video/`)
- [ ] Docs

## Checks

- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` pass
- [ ] Translations keep every `{placeholder}` and `{g:…}` exactly as in English
- [ ] A website change was made in `tools/site/` and `python tools/site/build.py` was run (CI compares `docs/`)
- [ ] Nothing from the game is added: no game files, no text, no images, no survey or locale output
- [ ] If it reads or writes the game: run against the real game, on Steam build `____`
