# 미니맵 3단계 — 지도 배경 조사 기록

미니맵에 배경 지도를 깔 수 있는지 조사한 기록. 결론과 선택지는 맨 아래.
(2026-10-02, Steam 빌드 24045435)

## 1. 에셋 열기

- pak/IoStore 는 AES 암호화 (utoc 플래그 Compressed|Encrypted|Signed|Indexed).
- **AES 키: 실행 파일에서 직접 찾음.** AESDumpster 방식 — `.text` 에서 같은 베이스 레지스터의
  연속 오프셋 8곳에 32비트 즉시값을 쓰는 코드(`C7 4x dd imm32`)를 찾아 후보 126개 →
  `pakchunk0-Windows.pak` 의 암호화된 인덱스 첫 블록을 각 후보로 풀어 FString 마운트 포인트
  (`int32 길이` + `../../../`)가 나오는 것이 **정확히 하나**.
- **키는 저장소·문서에 적지 않는다.** 필요하면 같은 방법으로 다시 찾는다 (스크립트는 아래 절차).
  사용자 PC 밖으로도 보내지 않는다.
- 도구: retoc v0.1.5 (https://github.com/trumank/retoc, 릴리스 zip sha256 확인).
  `retoc --aes-key <키> list --path --size <utoc>` 로 경로 목록,
  `retoc --aes-key <키> to-legacy --no-shaders --no-script-objects -f <이름> <Paks> <출력>` 으로
  필요한 에셋만 .uasset/.uexp 로.
- 목록 규모: pakchunk0 19,897 / pakchunk1 77,568 / pakchunk2 4,794 청크.

## 2. 지도 관련 에셋

| 에셋 | 내용 |
|---|---|
| `UI/Interaction/APC/WorldMap/APC_WorldMap_CloseBG_Img` | 15360×8640 DXT1, mip 1개 (66 MB) |
| `…/APC_WorldMap_MiddleBG_Img` | 7680×4320 DXT1 |
| `…/APC_WorldMap_FarBG_Img`, `…_Holder_Img` | 3840×2160 DXT1 |
| `GameData/StoryUnits/WMA_<지역>/…` | 지역(World Map Area)별 이름·설명·초상화·이동(Travel) 데이터 |
| `Editor/Tools/MiniMap/Icons/*` | 개발용 미니맵 도구의 아이콘 2개뿐 — 지역 지도 이미지는 출시 빌드에 없음 |
| `UI/HUD/HUD_CompassDial_*_Img` | 언어별 나침반 눈금 이미지 |

텍스처 디코딩: 쿠킹된 `.uexp` 에서 `PF_DXT1\0` 다음 12바이트(FirstMipToSerialize, NumMips,
벌크 플래그) 뒤가 바로 블록 데이터. 크기는 문자열 앞 SizeX/SizeY. BC1 은 numpy 로 풀었다.

## 3. 무엇이 들어 있나

`APC_WorldMap_*BG` 는 **장갑차(APC) 안에서 지역을 고르는 국가 전체 지도**(Hadea) — 도시
(Trisk, Kastel, Libane, Loblina, Lethe, Dalmask, Valde, Losilus, Yvel, Pyrean, Golmore),
지역 경계, 등고선. 탐험 지역(예: Senedra Forest)은 각자 **별도 월드**
(`SenedraForest_Root_WP`)라 이 지도의 좌표와 이어지지 않고, 지도 위에서는 작은 점 크기다.

→ **미니맵 배경으로는 쓸모가 적다.** 지역 안에서 길을 찾는 데 필요한 해상도도, 월드 좌표와의
대응도 없다.

## 4. 선택지 — **사용자 결정 (2026-10-02): B**

- **A. 국가 지도 오버레이:** 별도 단축키로 큰 창에 국가 지도를 띄우고, 지금 있는 지역(WMA)을
  표시. 미니맵 배경은 아님. 추출 이미지는 사용자 PC 에서 추출해 `Mods\` 에 둠 (배포 불가).
- **B. 게임 월드에서 직접 지도 만들기:** 로드된 레벨의 정적 메시(건물·바위·벽, 테스트 지역에서
  80 m 안에만 3,396개)의 위치와 크기(Bounds)를 위에서 본 윤곽으로 그려 미니맵 배경으로.
  에셋 추출 없이 실시간, 지역마다 자동. 2단계와 같은 액터 순회를 재사용.
- **C. 3단계는 접고 Phase 1 (UE4SS) 로.**

## 5. B 구현 기록 (2026-10-02)

- 재료(리플렉션으로 확인): 정적 메시 액터 9,696개 — 루트가 `StaticMeshComponent`
  (`RelativeLocation`, `RelativeRotation`(pitch, **yaw**, roll), `RelativeScale3D`, `StaticMesh`,
  `AttachParent`), 메시는 `UStaticMesh.ExtendedBounds` (+0x200, 56바이트: Origin·BoxExtent·Radius, double).
- `geometry.rs`: 메시 상자를 yaw 로 돌리고 스케일해 위에서 본 사각형(Footprint) + 높이 범위.
  붙어 있는(AttachParent 있는) 컴포넌트는 상대 좌표라 제외. 30 cm 미만·150 m 초과 제외.
  액터마다 한 번만 읽어 캐시, 3초마다 새로 로드된 액터만 추가. 첫 읽기 약 290 ms, 이후 거의 0.
- 그리기(`raster::draw_map`): 주인공 발(캡슐 중심 −90 cm) 기준 −1.5 m ~ +2.5 m 높이에 걸친 것만
  (천장·다른 층 제외). 높이 80 cm 미만은 바닥, 이상은 벽. 25 m 넘게 넓은 벽(절벽·큰 바위)은 상자가
  실제 모양보다 훨씬 커서 걸을 수 있는 땅을 덮으므로 제외.
- 첫 시도는 반투명 사각형을 그대로 겹쳐 그려 덩어리가 됐다 → 바닥·벽을 각각 마스크에 합집합으로
  그린 뒤 한 번만 일정한 투명도로 깔고, 마스크 가장자리에 윤곽선. 겹침이 쌓이지 않는다.
- 미리보기: `examples/preview.rs` (저장소 밖) 로 실제 게임 데이터 한 프레임을 이미지로 렌더링해
  확인. 테스트 지역(야외, z≈1261)에서 폐허의 벽·건물 윤곽이 깔끔하게 나옴.
- 패널 지도 탭 '벽·바닥 윤곽' 스위치 (minimap.txt `terrain`).
- **아직:** 게임 화면에서 확인, 던전(지하) 확인, 많은 메시가 있는 곳의 프레임 시간.

## 6. 사용자 피드백 1차 → 높이 층과 등고선 (2026-10-02)

- 피드백: 전체적으로 좋음. ① 반경을 줄였을 때 반경에 다 안 들어오는 지형 오브젝트가 안 그려짐
  ② 높낮이 구분이 어려움 — 고도선 같은 개선 필요.
- ① 재현해 보니 원 가장자리 자르기는 정상. 원인은 "폭 25 m 넘는 서 있는 메시 제외" 규칙 — 큰 지형일수록
  작은 반경에 다 안 들어오는데, 바로 그것들이 빠져 있었다. → 빼지 않고 '절벽·바위' 층으로 흐리게.
- ② 발 기준 높이로 층을 나눔 (`raster::band`): 깊은 곳(< −4 m) · 낮은 곳(−4 ~ −1.2 m) ·
  같은 높이(−1.2 ~ +0.6 m) · 높은 곳(+0.6 ~ +3 m) · 절벽·바위(25 m 넘는 서 있는 것) · 벽(내 높이에
  서 있는 것). 층마다 합집합 마스크 + 자기 색 윤곽선 → 높이가 바뀌는 곳에 선 = 등고선 역할.
  세로 범위를 −1.5 ~ +2.5 m 에서 −8 ~ +3 m 로 넓힘. 머리 위 평평한 것(천장)은 계속 제외.
- 지도 탭 '벽·바닥 윤곽' 아래에 층 색 범례.
- 미리보기로 확인: 계단·구덩이(파랑), 단·발판(모래색), 벽(밝은 선)이 구분됨. 절벽·바위는 상자가 커서
  넓은 직사각형으로 보이지만 흐려서 아래를 가리지 않음.
