# 시작 전 조사 (2026-10-01)

웹 조사 + 로컬 실행 파일 분석. Nexus·FearLess·PCGW 는 직접 접속이 403 이라 Nexus
GraphQL API, Wayback Machine, MediaWiki API 로 읽었다. 확인 못 한 것은 끝에 따로.

## 엔진·보호 장치

- UE **5.5.4** — https://www.pcgamingwiki.com/wiki/Hell_is_Us
- Denuvo 없음 — https://ixbt.games/en/news/2025/09/02/hell-is-us-vzlomali-do-reliza-no-nekotorye-xotiat-ee-kupit-iz-za-nebolsogo-vesa-failov.html ;
  PCGW: 실행 파일 직접 실행 시 DRM-free. 로컬 PE 섹션 정상, EAC 폴더 없음.
- `EOSSDK-Win64-Shipping.dll` 은 EOS 서비스용으로 추정 (안티치트 증거 없음).

## 기존 모드 (Nexus, 36개 전수 확인 — 치트·트레이너·지도·나침반 모드 없음)

| # | 이름 | 방식 |
|---|---|---|
| 43 | RE-UE4SS (UE 5.5 사전 설정) | `dwmapi.dll` 프록시, 엔진 버전 오버라이드만 필요 |
| 27 | Camera and Misc. | UE4SS Lua. 일반 UE4SS 에 `[EngineVersionOverride] 5/5` |
| 3 | Simple Mod Loader | 개발자 콘솔 + BP 모드 로더 |
| 22 | IOStore sideloader | `dsound.dll`, 서명 우회 → `~mods` pak 로드 |
| 33 | Mapping File | FModel 용 `.usmap` |
| 4 | FreeCam and Teleport | (README 미확인) |
| 16 | ReShade addon | FOV·카메라·개발자 콘솔 |
| 26 | Auto Healing Pulse | 화면 인식 외부 exe |

- Vortex 확장: https://www.nexusmods.com/site/mods/1417 — 서명 우회 필수, UE4SS 는
  `ue4ss/Mods`, BP pak 은 `Paks/LogicMods`.
- Universal Unreal Engine 5 Unlocker 동작 (PCGW).

## Cheat Engine 테이블 (FearLess)

- t=36338 (ndck1, 게임 v1.3.36): 인벤토리 편집기. 1.5+ 에서 안 된다는 보고.
- t=35282: matthew80 / Sianz / VampTY 테이블. 체력·스태미나·림빅·드론 쿨다운·질주 속도.
  - `StoryHero_BP_C` +0x688 → ASC, +0x1088 → SpawnedAttributes, +0x330 CharacterMovement.
  - 세트 순서 [0] Endurance [1] Lymbic [2] Damage [3] Weapon [4] Movement [5] Poise
    [6] Defense [7] 기타(XP·무적 시간·쿨다운).
  - 소모품 무한: `mov [rcx+48],edx` 패치 (아이템 수량 +0x48).
  - 초기 "무한 체력"이 적도 무적으로 만든 보고 — 공유 코드 훅의 부작용.
- 속성 이름 전체 목록은 이 테이블에서 (cheats.rs).

## 미니맵 구현 사례

1. Palworld PalMiniMap #3915 — UE4SS Lua 만으로 게임 UMG 에 월드맵 텍스처 + 아이콘.
   SceneCapture 없이 가볍게. https://github.com/jeankassio/PalMiniMap
2. Palworld Basic MiniMap #146 / YetAnotherMinimap #2973 — LogicMods BP pak.
   https://github.com/SvenBrnn/pal-yet-another-minimap-mod
3. Wukong Minimap #1172 — Rust + hudhook(DX12) + imgui, `dwmapi.dll` 프록시
   (UE4SS 와 충돌). 지도 없는 게임에 미니맵을 붙인 가장 가까운 선례.
   https://github.com/jaskang/wukong-minimap
4. Hogwarts Legacy Dynamic Compass #1063 — BP 로더 + UMG 나침반.
- 외부 투명 창 + 메모리 읽기 방식의 공개 사례는 못 찾음.

## pak·도구

- utoc: Compressed|Encrypted|Signed|Indexed. AES 키 공개된 것 없음 →
  AESDumpster (https://github.com/GHFear/AESDumpster) 로 로컬 추출 가능.
- retoc (https://github.com/trumank/retoc), UnrealReZen, FModel + #33 usmap.

## 로컬 실행 파일에서 찾은 것

- 클래스: `CharlieCharacterHero`, `CharlieHeroAbilitySystemComponent`,
  `CharliePlayerController`, `CharlieGameInstance`, `CharlieCheatManager`,
  `CharlieDebugMapCrawlerSubsystem`, `CharlieToggleDebugUI`, 치트 위젯
  (`CheatCategoryEntryWidget` 등) — 출시 빌드에 개발 치트 UI 가 남아 있음.
- 지도·나침반: `CompassSumg`, `ShowCompass`, `CompassOptions`,
  `Close/Middle/FarWorldMapSoftTexture`, `TeleportToMarker`, `TeleportMarkerComponent`.
- 속성 세트 클래스: ARCHITECTURE.md 참고.

## 확인 못 한 것

- Nexus 문서 탭(#3·#4 가 콘솔을 켜는 방법), FearLess Patreon 테이블.
- 재화·XP 속성 이름, AES 키, retoc 의 UE5_5 지원 여부.
- 공개 Dumper-7 SDK (Dumpspace 에 없음) — 필요하면 UE4SS 로 직접 덤프.
