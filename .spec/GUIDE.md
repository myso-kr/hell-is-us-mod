# 나침반 HUD 와 퀘스트 목표 안내 — 조사와 설계

사용자 요청 (2026-10-02): 나침반 HUD 를 화면 위 가운데에 고정하고, 퀘스트 목표까지의 경로
안내를 켜고 끄는 기능. 조사 결과, 결정, 구현 기록. Steam 빌드 24045435.

## 1. 게임 쪽 사실 (웹 조사)

- 웨이포인트·퀘스트 로그·맵 마커 없음이 설계 의도. 디렉터: "방향을 가리키는 마법 나침반도, 쇼핑
  리스트식 퀘스트 로그도 없다" — https://game8.co/articles/latest/hell-is-us-has-no-map-or-quest-markers-to-test-your-navigational-skills
- 게임 안 나침반은 아이템. 쓰면 화면 위에 방위가 잠깐 뜨고 사라짐, 목표를 가리키지 않음 —
  https://game8.co/games/Hell-is-Us/archives/545878
- Datapad: Investigations(메인 6개), People, Items, Locations, Research, Exploration(Good Deeds,
  Mysteries, Timeloops) — https://hellisus.wiki.fextralife.com/Investigations
- 지역 순서: Senedra Forest → Acasa Marshes → Vyssa Hills → Lake Cynon → Lethe → Talju → Marastan →
  Jeljin → Arcas Spire → Auriga Museum → Plains of Mist — https://hellisus.wiki.fextralife.com/Walkthrough
- 목표·마커를 보여 주는 모드는 없음 (Nexus 36개, GitHub, FearLess 확인). 공식 접근성 옵션에도 없음 —
  https://steamdeckhq.com/game-reviews/hell-is-us/
- 나침반 스트립 투영: 시야각 안이면 `x = tan(Δyaw)/tan(FOV/2)·W/2`, 시야보다 넓은 띠는 선형 —
  https://vazgriz.com/467/flight-simulator-in-unity3d-part-2/

## 2. 게임 메모리 쪽 사실 (로컬 조사, 읽기만)

### GUObjectArray
- `FUObjectArray` 가 이미지 +0x9304460 (청크 배열 포인터 +0x9304470), 객체 255,987개, 청크 4개.
  찾는 법: 쓰기 가능 섹션에서 (Objects, PreAllocated, Max, Num, MaxChunks, NumChunks) 모양이고 항목
  1..31 의 객체 `InternalIndex`(+0xC)가 제 슬롯과 같은 곳. 항목 0x18 바이트, 청크당 64K.
- 서브시스템은 리플렉션 밖이라 이것으로만 찾을 수 있다.

### 퀘스트는 "사실(Fact)" 지식 그래프
- `QuestData` (`Quest01_DA` …): `QuestElements`(관련 신원: 사람·장소·물건), `QuestLinks`(관계).
- `FactData` 계열(`QuestStatusData`, `TextFact`, `LinkFact`, `TypeFact`, `ImageFact` …):
  `bIsQuest`(+0x4c), `AssociatedIdentity`(+0x50), `AssociatedQuestData`(+0x58).
- **좌표 필드는 어디에도 없다.** 위치 사실도 텍스트(`Herbalist_SenedraForest_Location_TextFact`).
- 서브시스템: `QuestEventSubsystem`(월드), `FlowSubsystem`(Flow 그래프), `ResearchGameSubsystem`,
  `GoldenPathSubsystem`(CDO 만 — 출시 빌드에서는 안 만들어짐), `Bragi` 는 음악, `Bifrost` 는 지역 이동.

### 주인공의 지식 = 저장 상태
`CharlieSaveSubsystem.SaveSystem` → `CharlieSaveSystem.Saves` → `CharlieSaveGame` (슬롯마다 하나,
`SaveDate` 가장 새 것이 현재 플레이). `CharlieSaveGame.Player` (`CharlieSavePlayerState`, 1440 바이트):

| 필드 | 내용 (테스트 세이브) |
|---|---|
| `Knowledge.KnownFacts` | 아는 사실 124개 (`CharlieFactState.KnowledgeDataPath`, 소프트 경로) |
| `Knowledge.FactTags` | 태그 28개: `Quest.Facts.SenedraMedKitGiven`, `Secrets.Mystery.CaddellsBrothersTreasureStarted/Completed` … |
| `Datums.QuestStates` | 진행 중 조사: `Quest01_DA` (Family Reunion) |
| `Datums.DatumStates` | 알게 된 신원 10개 (부모, 약초꾼, OMSIF, APC, 지역 …) + 새 사실 |
| `SecretsState` | Good Deeds 26, Mysteries 43, Timeloops 14 (항목 상태) |
| `CurrentSavePoint`, `World.SeenCheckpoints` | 마지막 저장 지점, 들른 저장 지점 4개 |

소프트 경로(`FSoftObjectPtr`) = 약한 포인터 8 + 패키지 FName + 에셋 FName + 하위 경로 → 에셋 FName
인덱스로 객체 이름과 바로 비교할 수 있다.
**미확인:** 저장 객체가 플레이 중 실시간으로 갱신되는지, 저장할 때만 갱신되는지.

### 월드의 상호작용 오브젝트가 주는 것
`InteractableDynamicPayloadActor` 계열(아이템·퍼즐·트리거)에 붙은 컴포넌트:
- `PayloadRuneComponent.Rune.PayloadData`: `ContainedFacts`(TSet<FactData*>), `BaseIdentity`,
  `TagFacts`(GameplayTagContainer), `ItemsToAdd` … — **쓰면 받는 사실·태그**.
- `WorldLocationRuneComponent.Rune.WorldLocation` (FVector) — 자기 위치.
- `QuestListener` 액터: 퀘스트 상태 사실을 참조(`FactTag_Quest02Started`)하고 이벤트를 받음.

## 3. 결론 — 목표 위치는 계산할 수 있다

게임은 목표 좌표를 갖고 있지 않지만, **"주인공이 아직 모르는 사실·태그를 주는 오브젝트"의 위치**는
알 수 있다. 그중:
- 사실의 `AssociatedQuestData` 가 진행 중 조사이거나 태그가 `Quest.` 로 시작 → **퀘스트 목표**
- 태그가 `Secrets.` 로 시작 → **비밀(미스터리·선행·타임루프)**
- 나머지 → **단서**(기록물·연구 자료 등)
이미 쓴 오브젝트(`bHasBeenActivated`)는 2단계와 같이 뺀다.

한계: 대화로 얻는 사실(NPC)은 오브젝트가 아니라 빠진다. 지역 사이 이동 목표(다른 월드)는 안 보인다.

## 4. 설계

- **나침반 HUD:** 게임 창 위 가운데에 고정된 클릭 통과 띠(레이어드 창, 미니맵과 같은 방식).
  카메라 yaw 기준 ±90° 를 선형으로, 15° 눈금, N/E/S/W, 각도. 핀: 안내 대상(강조 + 거리),
  사용자 마커, 저장 지점, 퀘스트 목표. 범위 밖 대상은 양 끝에 화살표.
- **안내:** 패널 '안내' 탭 — 대상 목록(퀘스트 목표 / 비밀 / 단서 / 마커 / 저장 지점, 거리순),
  하나 고르면 나침반 핀 강조 + 미니맵에 주인공→대상 선. '가장 가까운 퀘스트 목표 자동 안내' 스위치,
  대상 순환 단축키, 안내 끄기.
- **경로:** 1차는 직선 방향·거리. NavMesh(Recast) 경로는 바깥에서 dtNavMesh 를 읽는 공개 사례가 없어
  다음 단계 후보로 남긴다.

## 5. 구현 기록 (2026-10-02)

| 파일 | 하는 일 |
|---|---|
| `names.rs` | 구조체 리플렉션: `struct_of`(StructProperty → ScriptStruct), `inner_of`(배열 안쪽 속성), `field_type`(FFieldClass 이름), `path`(객체 → 구조체 안 필드 경로) |
| `gobjects.rs` | GUObjectArray 찾기(유일해야 함), 모든 객체, 클래스 이름으로 객체 찾기 |
| `knowledge.rs` | 가장 새 `CharlieSaveGame` → 아는 사실·태그·진행 중 조사 (FName 인덱스 집합) |
| `goals.rs` | 상호작용 오브젝트의 페이로드(사실·태그) 1회 읽어 캐시, 지식과 비교해 퀘스트/비밀/단서 분류, 쓴 것 제외 |
| `engine.rs` | `Attached::goals` — GUObjectArray 1회, 저장 슬롯 60초, 지식 2초, 페이로드 2초 주기 |
| `raster.rs` | 획 글꼴(N E S W 숫자 m k . -), `draw_compass`(±90° 선형, 15° 눈금, 8방위, 핀, 대상 거리), 미니맵에 목표 마름모·대상 점선 |
| `ui/layered.rs` | 레이어드 창 공용 (미니맵·나침반) |
| `ui/minimap.rs` | 오버레이 스레드: 미니맵 + 나침반 + 안내 대상 고르기(`settle_target`, `cycle`) |
| `ui/panel.rs` | '안내' 탭: 나침반 켜기·키, 자동 안내, 종류 칩, 현재 대상, 진행 중 조사, 갈 곳 목록(거리순, 눌러서 안내) |

- 키(기본): 미니맵 F9, 마커 F6, **나침반 F10, 다음 목표 F11** — 넷 다 패널에서 바꿈, 겹치면 모두 기본값으로.
- 자동 안내: 고른 곳이 없거나 사라지면(써서 없어짐) 가장 가까운 퀘스트 목표로.
- 방위: N = 월드 +X (미니맵과 같음). 게임 나침반 아이템의 북쪽과 같은지는 **미확인**.
- 실측(테스트 세이브, Senedra Forest): 사실 124 · 태그 28 · 조사 `Quest01_DA` → 갈 곳 11곳
  (퀘스트 3: APC 문 열기 `Quest.Facts.APCAcquired` 90 m, Arcas Spire 문, Arcas Spire 책 /
  비밀 6: 림빅 문 2, 피의 여왕 창고, 타임루프 시작·완료, 밀수꾼 피란민 / 단서 2). 첫 읽기 ~1초.
- 미리보기(`examples/preview.rs`, 저장소 밖)로 실제 데이터 한 프레임 확인: 나침반 방위·대상 거리,
  미니맵 점선 방향이 서로 맞음.
- **아직:** 게임 화면 확인, 저장 상태가 실시간인지(아이템을 주운 직후 목록에서 빠지는지),
  대화로 얻는 사실(오브젝트 아님)은 목표로 안 나옴, 다른 지역(월드) 목표는 안 보임, NavMesh 경로는 다음.
