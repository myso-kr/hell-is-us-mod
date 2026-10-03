# Website (GitHub Pages, docs/)

`docs/` is generated output. Edit the sources in `tools/site/`, then run `python tools/site/build.py`.
The one exception is `docs/assets/media/`, which `tools/video/render.mjs` writes directly (see Video).

| File | Contents |
|---|---|
| `tools/site/template.html` | Single semantic HTML page with `{{key}}` placeholders. An unknown key fails the build. |
| `tools/site/strings/<lang>.json` | Copy for 12 languages; a missing key falls back to English. Each language was written by a native-speaker persona agent using the game's official terms. |
| `tools/site/site.css` | Same palette as the panel (`src/ui/theme.rs`). |
| `tools/site/site.js` | three.js hero, spoiler demo, entrance animations. |
| `tools/site/config.json` | Site and repository URLs (`base_url`, `repo_url`, `releases_url`, `license_url`) and `video_date`. Like the sibling repositories, `myso-kr/hell-is-us-mod` → https://myso-kr.github.io/hell-is-us-mod/ |
| `tools/site/og.png` | Social preview (1200×630, hero screenshot). |
| `tools/site/favicon.svg` | Favicon. |

What `build.py` writes:

- `docs/index.html` (English, also x-default) and `docs/<lang>/index.html` for the other 11 languages.
- `docs/sitemap.xml`, `robots.txt`, `llms.txt`, `.nojekyll`.
- `docs/assets/`: `site.css`, `site.js`, `favicon.svg`, `og.png`, and copies of the mod's `assets/icons`,
  `assets/symbols` and `assets/pins` (these three folders are deleted and recopied on every build).
- It does not touch `docs/assets/media/`, so the video exists in the repository only once.

## Design

- **Concept:** "No map. No compass. Unless you want one." The motif is the terrain contour lines of the
  game's art direction. The hero is a three.js terrain drawn as contour lines only by a shader, set in
  fog; a Lymbic-blue path traces the valleys to a beacon (the goal). It shows the minimap and route
  guidance instead of describing them. A compass strip, which the game does not have, runs across the
  top. The camera responds slowly to pointer and scroll.
- **Type:** Fira Sans Extra Condensed for headings (close to the game UI's condensed sans) and IBM Plex
  Sans for body text; Noto Sans KR/JP/SC for CJK (loaded only on those language pages).
- **References:** researched examples of one-shot Opus page prompts: focus on one memorable moment (the
  hero) and keep the rest quiet; avoid the stock defaults (cream background, italic emphasis, 01/02
  labels); respect `prefers-reduced-motion`; verify at 1440 and 390 px widths.
- **Accessibility:** skip link, landmarks, `aria-labelledby`, focus rings, a still frame under reduced
  motion, CSS contour lines when WebGL is unavailable.

## SEO, AEO, GEO

- One static page per language (`/`, `/ko/`, …). All content is in the HTML, readable without the 3D
  scene. 12 `hreflang` alternates plus x-default, canonical, OpenGraph/Twitter tags with locale and
  alternate locales.
- JSON-LD, in the page's language: `SoftwareApplication` (about: `VideoGame` Hell Is Us, Rogue Factor /
  Nacon, Steam app 1620730), `FAQPage` (6 questions), `HowTo` (3 install steps), `VideoObject`,
  `WebSite`.
- FAQ questions are phrased the way people actually ask them ("Does Hell Is Us have a map?", "Where
  are the vault codes?"), with self-contained answers that answer engines can quote.
- `sitemap.xml` (with `xhtml:link` language alternates), `robots.txt`, `llms.txt` (a plain summary for
  language models: features, FAQ, links, and an unofficial-fan-tool notice).
- No game images or assets (trademark and copyright). Icons and symbols are SVGs drawn for the mod.

## Deployment

1. Run `python tools/site/build.py` and commit.
2. GitHub repository Settings → Pages → Deploy from branch → `main` / `/docs`. Pages on a private
   repository needs a paid plan (Pro or above).
3. Check locally with `python -m http.server -d docs`, and take headless Edge screenshots at 1440 and
   390 px widths.

## Performance (2026-10-03)

- **Two resolutions:** the terrain (the contour shader, nearly all of the cost) renders to a target at
  0.66× CSS pixels and is upscaled with linear filtering. The path and beacon are drawn on top at
  screen resolution (up to 1.5×, MSAA). The terrain depth is refilled at full resolution with color
  writes off, so hills still hide the path behind them. MSAA applies only to the full-resolution passes
  (upscale, depth, path), so it is cheap. Capped at 30 fps.
- **Adaptive:** if the drawn frame interval exceeds 1.5× the target 20 times in a row, the scale drops by
  0.15 (minimum 0.45); after 240 fast frames it rises by 0.1 (maximum 0.85).
- **Geometry:** terrain 160×160 (was 220×220, about 50,000 triangles; at 120 or below the contours bend
  visibly at every quad), path tube 220×4, far plane 230 m.
- Nothing is drawn while off-screen or while the tab is hidden. Rendering starts at idle after first
  paint, so it never blocks the text; with Save-Data only the CSS contours are used.

## Video (2026-10-03)

A 31-second silent introduction.

- **Scene:** `tools/video/scene.html`, a single page where every pixel is a function of `t`
  (`window.seek(t)`, `window.DURATION = 31`): no clock, no `requestAnimationFrame`, no randomness.
- **Render:** `tools/video/render.mjs` serves the repository on a local port, opens the scene in
  headless Edge (puppeteer-core; Chrome via `CHROME=path`) at 1920×1080, and for each frame seeks,
  screenshots, and pipes the PNG into ffmpeg; no frames are written to disk. The same scene always
  renders the same video. Requires Edge or Chrome and ffmpeg on `PATH`.
- **Commands:** `cd tools/video && npm install && npm run render`. `node render.mjs --only poster` (or
  `--only video`) renders one part; `--stills 1,5,10` writes those keyframes to `tools/video/stills/`
  for checking first; `--fps` defaults to 30.
- **Outputs:** frames are first encoded to a high-quality master, `tools/video/master.mp4` (H.264
  CRF 14, not committed), which is then re-encoded straight into `docs/assets/media/`:
  - `hiumod-intro.mp4` (H.264 CRF 24, faststart, no audio);
  - `hiumod-intro.webm` (VP9, CRF 38);
  - `poster.jpg` (frame at 5.2 s).
  `tools/video/.gitignore` excludes `node_modules/`, `master.mp4` and `stills/`.
- **No GIF** (file size; the user's decision). The README shows `poster.jpg` linked to the WebM on the
  site. GitHub READMEs do not play video files stored in the repository inline (only `user-attachments`
  URLs uploaded through the web UI), so this could switch to that once the repository is public.
- **Storyboard:**
  - 0–7 s: "No map. No compass. / Unless you want one." with the compass strip.
  - 7–10 s: the camera rises and the terrain becomes a map (fog thins); icons appear.
  - 10–13 s: it shrinks into a round minimap on the right.
  - 13–18 s: a quest tracker card, with the distance counting down.
  - 18–23 s: an example vault; the cursor clicks "Show answer" and the symbols appear one by one.
  - 23–27 s: the word "map" in 12 languages.
  - 27–31 s: logo and URL, fade to dark (looping playback continues from the start).
- **Nothing from the game:** the terrain is noise, icons and symbols are the mod's SVGs, and the text is
  generic (no real quest names).
- **Research:** a deterministic HTML → frames → ffmpeg pipeline with Opus 5.5 (Hugging Face blog) versus
  the Remotion approach (React components, `useCurrentFrame`). The former was chosen for fewer
  dependencies.
- **On the site:** `.film`, directly below the hero: `muted loop playsinline`, `preload="none"` with the
  poster, plays only while visible; under reduced motion it shows controls only. JSON-LD `VideoObject`
  (`uploadDate` from `config.json` `video_date`, duration `PT31S`). Strings `video_title`,
  `video_label`, `video_caption` in 12 languages.
