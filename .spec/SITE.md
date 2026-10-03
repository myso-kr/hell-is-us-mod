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
