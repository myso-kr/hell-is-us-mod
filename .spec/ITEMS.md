# 퀘스트 아이템 전수조사 — 설계 (2026-10-03)

요청: "퀘스트 아이템을 전수조사해서 위치를 전부 조사하자. 이게 없어서 입구를 가더라도 아이템이 어디 있는지
찾을 수가 없어."

## 1. 왜 지금은 못 찾나

목표(goals.rs)는 **지금 메모리에 로드된 액터**에서만 나온다. World Partition 은 플레이어 주변 셀만 스트리밍하므로:
- 로드 범위(대략 80–200 m) 밖의 아이템·NPC·장치는 존재 자체를 모른다.
- 다른 지역(별도 월드)에 있는 아이템은 전혀 모른다.
- 문·퍼즐 너머(지하 등)는 셀이 로드돼도 "어디서 열쇠를 얻는지"를 모른다.

→ 게임 데이터 **전체**에서 미리 조사한 목록(오프라인 DB)이 필요하다.

## 2. 데이터 원천 — 실측

| 원천 | 내용 | 확인 |
|---|---|---|
| pak(IoStore, AES) `Maps/<지역>/<지역>_Root_WP/_Generated_/*.umap` | 쿠킹된 WP **스트리밍 셀** — 모든 배치 액터가 여기 | 4,043 셀: LakeCynon 868 · LethePropaganda 788 · LetheLibrary 550 · SenedraForest 520 · Marastan 310 · AcasaMarshes 253 · PlainsOfMist 242 · VyssaHills 202 · AurigaMuseum 108 · Jeljin 102 · Talju 99 |
| 같은 pak `Maps/<지역>/*.uasset` (Town, TownSecrets, SophieHouse …) | 레벨 인스턴스/데이터 레이어용 하위 레벨 | 경로만 확인 |
| 셀 안 액터의 컴포넌트 | `PayloadRuneComponent`(ItemsToAdd·ContainedFacts·TagFacts), `SaveIdentifierRuneComponent`(Guid), `FlowComponent`(RootFlow), `TradeGiveItemRuneComponent`(ValidTrades), `InteractionActionComponent` | 런타임 리플렉션으로 구조 확인(GUIDE §3, QUESTS §5) |
| 세이브 `World.RegionStates[]` | 지역(LevelPath)별 `ElementStates[]` = {Identifier: Guid, RuneStates, Data} — **배치 오브젝트의 상태를 Guid 로** | Senedra 123개 실측 |
| 세이브 `KnownFacts`/`FactTags`/인벤토리 | 이미 아는 사실·태그, 가진 아이템 | 기존(knowledge.rs, engine inventory) |
| 아이템 에셋 경로 | `/Items/Quests/QuestNN/…`, `/Items/Secrets/<지역>/…` | QUESTS §1, goals.rs |

AES 키: 실행 파일에서 찾는 방법이 MAP.md §1 에 있음(키는 어디에도 적지 않음). 도구: retoc(이미 사용),
**dotnet 8 SDK 설치돼 있음** → CUE4Parse(FModel 의 엔진, IoStore·UE5.5·unversioned 지원) 사용 가능.

## 3. 전체 구조

```
[게임 pak] --(오프라인 조사기, 사용자 PC에서 1회/패치마다)--> Mods\survey\*.json
                                                              │
[게임 메모리] --(hiumod 런타임)-- 지식·세이브 상태·현재 월드 ---┼--> 목표(가까운 곳=실시간, 먼 곳=DB)
                                                              └--> 추적기·패널·나침반·경로
```

### 3.1 오프라인 조사기 `tools/survey` (C# .NET 8 + CUE4Parse)
- 입력: Paks 폴더, AES 키(실행 시 hiumod 이 찾아 넘김 — 파일로 저장하지 않음), **매핑(.usmap)**.
- 매핑: 쿠킹 에셋은 unversioned 프로퍼티라 스키마가 필요 → **`hiumod doctor usmap`** 을 새로 만들어 런타임
  리플렉션(이미 전부 읽고 있음)에서 `.usmap` 을 쓴다. 블루프린트 클래스는 CUE4Parse 가 에셋의 BPGC 에서 읽음.
- 처리: 각 `_Root_WP` 의 모든 셀 umap → 액터 export 별로
  - 클래스(import 경로), 루트 컴포넌트 `RelativeLocation`(셀 액터는 월드 좌표), 셀 이름
  - `SaveIdentifierRune.Identifier`(Guid) — **세이브와 맞추는 열쇠**
  - `PayloadRune`: ItemsToAdd(아이템 경로), ContainedFacts(사실 경로), TagFacts(태그)
  - NPC: `FlowComponent.RootFlow` 경로 → 그 플로우 에셋(+주제 서브그래프)의 `FlowNode_Payload` 내용
  - `TradeGiveItemRune.ValidTrades`: 원하는 아이템 + 보상 페이로드
  - 클래스 이름의 `_PayloadInactive_`(조건/방문), `GatherSingleUse`(줍기) 구분
- 사실 에셋(`FactData`)은 따로 읽어 `AssociatedQuestData`·Track·Description(FText 원문 키) 를 붙임.
- 출력: `Mods\survey\<World>.json` + `Mods\survey\facts.json` — **커밋·배포하지 않음**(게임 데이터에서 나온 것).
  빌드 id 를 같이 적어 패치되면 다시 돌리라고 알림.

### 3.2 런타임 `src/survey.rs`
- 시작 시 `Mods\survey` 를 읽어 월드별 색인. 없으면 지금처럼 실시간 목표만(기능 저하 없음).
- **상태 판정**(무엇이 아직 남았나):
  1. 세이브 `RegionStates[현재 월드].ElementStates` 에 그 Guid 가 있고 Data 가 "사용됨"이면 끝(Data 해석은 실측 필요 —
     줍기 전/후 세이브 비교로 확정).
  2. 페이로드의 사실·태그를 이미 알면 끝(지금 goals.rs 와 같은 규칙).
  3. 아이템을 이미 가졌으면 끝(인벤토리).
  4. 로드된 범위 안이면 실시간 값(`bHasBeenActivated`)이 우선.
- **목표 합치기**: 실시간 목표 + DB 목표(같은 Guid 는 하나로). DB 목표는 `Goal` 에 `source: Survey` 와 셀 이름을 붙임.
- **퀘스트 연결**: 아이템 `QuestNN_` → 그 메인 퀘스트, 사실의 `AssociatedQuestData`, 선행은 보상 태그 접두 / 전달
  아이템. 이로써 "이 퀘스트에 필요한 아이템 전부 + 각각 어디 있는지 + 이미 얻었는지" 목록이 나옴.
- **다른 지역**: 목표가 다른 월드면 추적기에 "다른 지역: <WMA 이름> — 장갑차로 이동", 패널 목록에 지역별로.

### 3.3 안내
- 같은 월드 DB 목표가 로드 범위 밖이면: 내비메시 경로를 닿는 가장 가까운 곳까지 + 나머지 점선(지금의 막힘 처리 재사용),
  다가가면 셀이 로드되어 실시간 목표로 바뀌며 정확해짐.
- 문·퍼즐 너머 목표: 같은 퀘스트의 열쇠·쪽지 아이템(DB)을 먼저 — "helper"(GUIDE §20)를 DB 까지 넓힘.
- 패널: 퀘스트 카드에 **"필요한 것"** 목록 — 아이템/NPC/장치, 지역, 거리, 상태(남음/얻음), 눌러서 안내.
- 지도: DB 목표도 다이아몬드로(로드 전엔 테두리만), 다른 층은 흐리게(§18 재사용).

## 4. 단계와 검증 (단계마다 사용자 확인)

| 단계 | 할 일 | 확인 방법 |
|---|---|---|
| S1 | `hiumod doctor usmap` — 리플렉션 → .usmap | CUE4Parse/FModel 이 Charlie 클래스를 읽는지 |
| S2 | 조사기 골격: pak 열기·셀 umap 목록·액터 클래스/위치 출력(1개 지역) | Jova(Acasa) 의 알려진 오브젝트 좌표가 런타임 값과 일치(예: `Quest01PictureC`) |
| S3 | 페이로드·Guid·NPC·거래 추출, 전 지역 JSON | 런타임 goals 와 대조: 로드 범위 안의 목표가 DB 에 전부 있는지 |
| S4 | 세이브 상태 판정: 줍기 전/후 세이브 비교로 `ElementStates.Data` 의미 확정 | 아이템 하나 주워서 "얻음" 으로 바뀌는지 |
| S5 | 런타임 합치기·퀘스트 연결·패널 "필요한 것"·다른 지역 안내 | 가족 재회: 남은 아이템 전부가 위치와 함께 나오는지 |

## 5. 위험

- **데이터 레이어**: 일부 액터는 스토리 단계별 데이터 레이어에서만 활성 — 셀에 있어도 지금은 없을 수 있음. 셀/액터의
  데이터 레이어를 같이 뽑아 두고, 런타임에서 그 레이어가 켜졌는지(로드됐는지)로 거름. (S3 에서 조사)
- **Data 의미 미확정**: ElementStates.Data 형식을 모르면 1번 판정 대신 2–4번만 씀(사실·태그·인벤토리로 대부분 판정 가능).
- **CUE4Parse 의 UE5.5/게임 커스텀 버전 호환**: S2 에서 바로 드러남. 안 되면 retoc to-legacy + 자체 파서(Rust, 필요한
  컴포넌트 몇 종만)로 대체 — 리플렉션이 있어 스키마는 확보됨.
- **배포**: 조사 결과·키·추출물은 저장소에 넣지 않음. 조사기는 저장소에(코드만), 실행은 사용자 PC 에서.

## 6. 결정할 것

1. 조사기 구현: **C# + CUE4Parse (권장 — 검증된 파서, 빠름)** vs Rust 자체 파서(외부 의존 없음, 오래 걸림).
2. 범위: **퀘스트·선행 아이템 + 열쇠·쪽지 + 페이로드 있는 장치·NPC (권장)** vs 모든 줍기(소모품·재료 포함).

## 7. 진행 기록 (2026-10-03)

### S1 — 매핑 ✅
- `hiumod doctor usmap` → `Mods\doctor\HellIsUs.usmap` (usmap v0, 비압축). 로드된 클래스·구조체 14,224 + 열거형 2,249,
  1.8 MB. UEnum 이름 표 = +0x40 (FName, i64) 쌍, `Enum::` 접두 제거. FProperty ArrayDim +0x30, 하위 타입 포인터 +0x70~.

### S2 — 조사기 골격 ✅
- `tools/survey` (C# .NET 8 + **CUE4Parse 1.2.2** — 최신 1.2.2.2026xx 는 .NET 10 전용이라 net8 의 마지막 판, UE5_5 지원).
  Oodle 은 retoc 가 받아 둔 `oo2core_9_win64.dll`. 설치 추가 없음.
- **배치 액터는 셀(`_Generated_/*.umap`)만이 아니라 `<지역>_Root_WP.umap` 에도 있음**(항상 로드되는 것 — 퀘스트 아이템
  다수가 여기, Acasa 178 중 122).
- **컴포넌트 값은 대개 블루프린트 템플릿에** — 배치본의 `Template` → 그 `Template` … 체인을 합쳐 읽음(예: PayloadRune 의
  ItemsToAdd 는 `Quest01PictureC_…_BP_C:PayloadRune_GEN_VARIABLE` 에).
- **위치는 부착 체인 합성** — 루트 컴포넌트가 다른 액터에 붙어 있으면(`AttachParent`, 같은 패키지의 export 번호) 상대
  좌표라, 부모들의 위치·회전(FRotator → 쿼터니언)·크기를 합성.
- 검증: Acasa 실시간 목표 위치와 **정확히 일치**(예: VitalisOfficeOpened (−7699, −13903, 506), Victor 대화 (−7094, −16770, 1310)).

### S3 — 전 지역 ✅
- 11개 월드, 2분 14초, 액터 1,174, 대화 그래프 361 (NPC → RootFlow → 주제/서브그래프 재귀).
- 퀘스트 아이템(줍기): Quest01 8 (Acasa 6, Senedra 2) · Quest02 28 (LakeCynon 16, Vyssa 12) · Quest03 15 (Marastan 10,
  Senedra 4, Talju 1) · Quest04 17 (Vyssa 10, LetheLibrary 6, Plains 1) · Quest05 24 (Auriga 12, Jeljin 6, LethePropaganda 6) ·
  Quest06 10 (LakeCynon). 선행·비밀 아이템 138. 대화가 주는 아이템 19, 그중 퀘스트 15(집 열쇠, 키카드, 도서관 열쇠 …).
- 출력 `Mods\survey\<월드>.json` = {name, class, cell, at, guid, payload{items, facts, tags}, flow, trades}, `flows.json` =
  {경로: {payloads, subgraphs}}. 커밋·배포 안 함.
- 데이터 레이어: 쿠킹된 액터에 `DataLayer*` 프로퍼티 없음(0) — 소속은 셀/런타임 해시 쪽. 필요해지면 조사.
- 실행(지금은 수동): `survey --paks <Paks> --usmap <usmap> --oodle <dll> --aes <키> --world all --out Mods\survey`.
  키 찾기를 hiumod 으로 옮겨 `hiumod survey` 한 번으로 돌게 하는 것이 S5 의 일부.

### S5 — 게임 안 연결 ✅ (S4 전: 획득 판정은 지식·인벤토리로)
- **`hiumod doctor survey [world]`**: 매핑 → `tools/survey` 실행. AES 키 찾기는 C# 조사기로 옮김(`AesKey.cs`, .NET 내장
  AES — 파이썬 스크립트와 같은 방법). Oodle 은 `Mods\tools\oo2core_9_win64.dll`(없으면 CUE4Parse 가 내려받음).
  `--game <설치 폴더>` 로 나머지 경로가 정해짐. 한 지역만 돌려도 `flows.json` 은 합쳐 둠.
- **`src/survey.rs`**: `Mods\survey` 를 읽어 월드별 항목(이름·클래스·위치·아이템·사실·태그·원하는 아이템, NPC 는 대화
  그래프 전체 페이로드를 합침). 쿠킹 이름 끝 숫자(`_UAID_…_1325171520`)는 런타임엔 없어 떼고 맞춤.
- **남은 것 판정**(`Entry::left`): 모르는 사실, 모르는 태그(`Conversation.` 제외), 안 가진 아이템 — 단 사실·태그가 다
  알려졌으면 아이템은 얻은 것으로(열쇠·쪽지는 쓰고 나면 인벤토리에서 사라짐).
- **목표 합치기**: 지금 월드의 조사 항목 중 로드된 것(goals.rs 가 이름을 모음)은 실시간 목표에 맡기고, 나머지 중 남은
  게 있는 것을 목표로(id 최상위 비트 = 조사 항목). 실측(Acasa, Jova): 실시간 57 + 조사 22.
- **퀘스트 연결**: 아이템 경로 `/Items/Quests/QuestNN/` → QuestNN, 사실 이름 → 그 메인 퀘스트(로드된 QuestData 의 사실),
  선행은 태그 접두.
- **필요한 것**(`Snapshot.needs`): 진행 중인 퀘스트마다 모든 월드의 항목과 남았는지. 실측 가족 재회: 12곳 중 8곳 남음
  (Acasa 6: 수수께끼 쪽지·사진 C·UN 여행 지도·림빅 자물쇠 쪽지·빅터의 집 열쇠 대화 2, Senedra 2).
- **패널**: 퀘스트 카드에 "필요한 것 — 남은 n / m", 이 지역 것 가까운 순 8개(눌러서 안내 — 로드된 것이면 실시간 목표로),
  다른 지역 개수(장갑차로 이동). **추적기**: "필요한 것: 이 지역 n곳 · 다른 지역 m곳".
- 한계: NPC 는 대화 그래프의 모든 갈래를 합치므로 다 알기 어려워 "남음" 으로 오래 남을 수 있음. S4(세이브의 배치 상태)
  로 보완 예정.

### S4 — 세이브의 배치 상태 ✅ (2026-10-03, 사용자가 사진 C 를 주움)
- `CharlieSaveGame.World.RegionStates[]`(원소 0x80: LevelPath, Elements 맵, **ElementStates** +0x70) →
  `CharlieSaveWorldElementState`(0x70: Identifier Guid, RuneStates 맵 +0x10, Data +0x60 = TArray<InstancedStruct>).
- 실측(Acasa 64개): 주운 사진 A·C 는 자기 GUID 로 `PersistentActorSaveGameState`(ActorData 14 B, ComponentsData 1 B —
  둘 다 똑같음: "소비됨" 표시)가 있고, 안 주운 수수께끼 쪽지·UN 지도·자물쇠 쪽지는 **기록이 없음**. 사진 C 는 줍자 생김.
  나머지는 `SerializeSpawnerState`(적 스포너).
- 규칙: 조사 항목의 GUID 가 세이브에 있으면 = 가져갔거나 썼음 → 남은 것 없음. NPC 는 예외(한 번 말 건 것뿐일 수 있음).
- 결과(가족 재회): 12곳 중 남음 8 → 6 (사진 C, Senedra 응급 키트가 "얻음" 으로 바로잡힘).
