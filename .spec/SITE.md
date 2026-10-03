# 홈페이지 (GitHub Pages, docs/)

`docs/` 는 생성물입니다. 고칠 곳은 `tools/site/`, 만든 뒤 `python tools/site/build.py`.

| 파일 | 내용 |
|---|---|
| `tools/site/template.html` | 한 장짜리 시맨틱 HTML, `{{키}}` 자리 — 모르는 키가 있으면 빌드 실패 |
| `tools/site/strings/<lang>.json` | 12개 언어 문구 (빠진 키는 영어로). 언어별 원어민 페르소나 에이전트가 게임 공식 용어로 작성 |
| `tools/site/site.css` | 패널과 같은 팔레트 (`src/ui/theme.rs`) |
| `tools/site/site.js` | three.js 히어로 + 스포일러 데모·등장 애니메이션 |
| `tools/site/config.json` | 사이트·저장소 주소 — 형제 저장소처럼 `myso-kr/hell-is-us-mod` → https://myso-kr.github.io/hell-is-us-mod/ |
| `tools/site/og.png` | 공유 미리보기 (1200×630, 히어로 스크린샷) |

## 디자인
- 콘셉트: "지도도 나침반도 없다 — 원한다면." 게임 아트 디렉션의 지형 등고선 모티프. 히어로는 three.js 지형을
  셰이더로 등고선만 그리고 안개 속에 두며, 골짜기를 따라 림빅 블루 경로가 그려져 끝의 신호등(목표)까지 감 —
  미니맵·길안내를 말 대신 보여 줌. 위쪽엔 게임에 없는 나침반 띠. 포인터·스크롤에 카메라가 천천히 반응.
- 서체: Fira Sans Extra Condensed (제목, 게임 UI 의 압축 산세리프 느낌) + IBM Plex Sans (본문), CJK 는 Noto Sans KR/JP/SC.
- 참고: 웹 리서치한 Opus 원샷 프롬프트 사례 — "한 가지 기억할 순간(히어로)에 집중, 나머지는 조용히",
  기본값(크림 배경·이탤릭 강조·01/02 라벨) 금지, prefers-reduced-motion 존중, 1440/390 폭 검증.
- 접근성: 건너뛰기 링크, 랜드마크, aria-labelledby, 포커스 링, reduced-motion 이면 정지 프레임, WebGL 없으면 CSS 등고선.

## SEO · AEO · GEO
- 언어별 정적 페이지 (`/`, `/ko/` …) — 3D 없이도 본문 전부 HTML. `hreflang` 12개 + x-default, canonical, OG/Twitter (locale·alternate).
- JSON-LD: SoftwareApplication(about: VideoGame Hell Is Us), FAQPage, HowTo(설치 3단계), WebSite — 언어마다 그 언어로.
- FAQ 는 사람들이 실제로 묻는 꼴 ("Hell Is Us 에 지도가 있나?", "금고 코드는 어디?") + 자기완결 답 → 답변 엔진 인용용.
- `sitemap.xml` (xhtml:link 대체 언어), `robots.txt`, `llms.txt` (LLM 용 요약 — 기능·FAQ·링크), `.nojekyll`.
- 게임 이미지·에셋 미사용 (상표·저작권). 아이콘·문양은 모드용으로 그린 SVG.

## 배포
1. `python tools/site/build.py` → 커밋.
2. GitHub 저장소 Settings → Pages → Deploy from branch → `main` / `/docs`.
   비공개 저장소는 Pages 에 유료 플랜(Pro 이상) 필요.
3. 확인: 로컬 `python -m http.server -d docs`, Edge headless 로 1440·390 폭 스크린샷.

## 성능 (2026-10-03)
- 두 해상도: 지형(등고선 셰이더 — 비용의 거의 전부)은 CSS 픽셀 0.66 배 렌더 타깃에 그려 선형 보간으로 확대.
  경로선·신호등은 화면 해상도(최대 1.5×, MSAA)로 그 위에 — 지형 깊이만 원해상도로 다시 채워(색 쓰기 끔) 언덕 뒤는 가려짐.
  MSAA 는 원해상도 패스(확대·깊이·경로)에만 걸려 싸다. 30 fps 상한.
- 적응형: 그린 프레임 간격이 1.5배 넘는 게 20번 이어지면 배율 −0.15 (최저 0.45), 빠른 게 240번이면 +0.1 (최고 0.85).
- 지형 160×160 (전 220×220, 약 5만 삼각형 — 120 이하는 등고선이 사각형마다 꺾임), 경로 튜브 220×4, 원거리 230 m.
- 화면 밖·탭 숨김이면 그리지 않음. 첫 페인트 뒤 idle 에 시작 (본문 표시를 막지 않음), Save-Data 면 CSS 등고선만.

## 영상 (2026-10-03)
- 31 초 무음 소개 영상. `tools/video/scene.html` 한 장 — 모든 픽셀이 t 의 함수(`window.seek(t)`), 시계·rAF·난수 없음.
  `tools/video/render.mjs` 가 Edge(headless, puppeteer-core)로 프레임마다 seek → 스크린샷 → ffmpeg 파이프(디스크에 프레임 없음).
  `npm install && npm run render` → `docs/assets/media/` 에 (build.py 는 이 폴더를 건드리지 않음 — 저장소에 한 벌만) hiumod-intro.mp4 (H.264 CRF 24, faststart), .webm (VP9),
  poster.jpg. GIF 는 만들지 않음(용량, 사용자 결정) — README 는 poster.jpg 를 webm 링크로. GitHub README 는 저장소 안
  영상 파일을 인라인 재생하지 않음(웹 UI 로 올린 user-attachments 주소만) → 저장소 공개 후 그 방식으로 바꿀 수 있음. `--stills 1,5,10` 로 키프레임만 먼저 확인.
- 콘티: 0–7 s "No map. No compass. / Unless you want one." + 나침반 띠 → 7–10 s 카메라가 위로 올라가 지형이 지도가 됨(안개 옅어짐),
  아이콘 등장 → 10–13 s 원형 미니맵으로 줄어 오른쪽 → 13–18 s 퀘스트 추적기 카드(거리 줄어듦) → 18–23 s 예시 금고, 커서가
  "Show answer" 누르면 기호가 하나씩 → 23–27 s 12개 언어의 "지도" → 27–31 s 로고·URL, 어둠으로 (반복 재생이 이어짐).
- 게임 것 없음: 지형은 노이즈, 아이콘·기호는 모드 SVG, 문구는 일반적인 말(실제 퀘스트 이름 없음). 리서치: Opus 5.5 결정론적
  HTML→프레임→ffmpeg 파이프라인(Hugging Face 블로그), Remotion 방식(React 컴포넌트, useCurrentFrame) — 의존성을 줄이려 전자.
- 사이트: 히어로 바로 아래 `.film` — muted loop playsinline, preload none + poster, 화면에 보일 때만 재생,
  reduced-motion 이면 컨트롤만. JSON-LD VideoObject (`config.json` 의 video_date). 문구 video_title/label/caption 12개 언어.

