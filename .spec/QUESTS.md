# 메인 스토리 자동 길안내 조사 (2026-10-03)

요청: "현재 진행되는 선형 퀘스트를 순서대로 정렬해서 엔드게임까지의 길안내를 자동화하는 로직 — 조사."
두 갈래: 게임 데이터(doctor·pak 경로 목록, build 24045435) + 웹(공략).

## 1. 게임 데이터

### 메인 퀘스트 = `Quest01`–`Quest06`
- pak 경로: `Content/GameData/Quests/Quest0N/` (Quest01 33 · 02 20 · 03 18 · 04 16 · 05 19 · 06 8 에셋). 각 폴더:
  `Quest0N_DA`(QuestData), `Quest0N_Started_StatusFact_DA` / `Quest0N_Completed_StatusFact_DA`(Quest01 은 `Start_Status` /
  `Complete_Status`), `Links/*_LinkFact_DA`, `Quest0N_Name_DA`(StringFact — 표시 이름), 이미지·설명 사실.
  퀘스트 아이템은 `Content/Items/Quests/Quest0N/`(열쇠 등).
- **`QuestData`** (`FactOwnerData`): `IdentityName`("Quest01"), `MainQuestElement`·`QuestElements`(`QuestMindMapElement`
  {Identity, position} — 마인드맵의 인물·장소·물건), `QuestLinks`, `Links`(StoryUnit 쌍). 단계 목록이 아니라 **관계 그래프**.
- **`IdentityData`**: `IdentityName`("Victor Gaz"), `OwningStoryUnit`, `BaseFacts`(그 정체성에 붙는 사실들).
- **`FactData`**: `DebugName`, **`Priority`**(int), **`Track`**(FName — 주제), `bIsQuest`, `AssociatedIdentity`,
  **`AssociatedQuestData`**. 하위: `QuestStatusData`(AdditionalQuestStatus), `LinkFactData`, `StringFactData`, `ImageFactData` …
- **트랙과 우선순위**(실측, Quest01): 트랙 하나에 `…_Potential_LinkFact`(Priority 0) → `…_Confirmed_LinkFact`(Priority 1).
  예: `Quest01_VictorGaz` 0 ✓ → 1 ✓, `Quest01_DetainedPriest` 0 · 1 모두 모름, `Quest01_Tania` 0 모름.
  → **추정은 알고 확인을 모르는 트랙 = 다음 할 일**, 둘 다 모르는 트랙 = 아직 열리지 않은 갈래.
- 이 세이브: Quest01 사실 30개 중 20개 앎, Quest02 시작 전. Quest02·04·06 은 일부 사실만 로드되어 있음(에셋 스트리밍).

### 골든 패스 — 출시 빌드엔 없음
- `GoldenPathData.QuestSteps`(`QuestStep`{World, TeleportMarkerTag, StepDescription}), `GoldenPathSubsystem` 은 클래스만
  있고(CDO), 데이터 에셋은 pak 102,259 경로 중 없음 — 개발용 텔레포트 순서였던 것. 쓸 수 없음.

### 이미 있는 것 (GUIDE.md §2–5)
- 세이브의 `KnownFacts`/`FactTags`/`QuestStates` 를 읽고(knowledge.rs), 상호작용 오브젝트의 페이로드 사실 중 **아직 모르는 것**을
  주는 곳을 목표로(goals.rs, 등급 Quest/Secret/Clue). 퀘스트 등급 = 사실의 `AssociatedQuestData` 가 있음.

## 2. 웹 — 메인 줄기 (Fextralife, Game8, guided.news 등; 상세는 출처)

| # | 데이터 | 공략 제목 | 막 | 주 지역 (순서) | 다음을 여는 것 |
|---|---|---|---|---|---|
| 1 | Quest01 | Family Reunion | 1 | Senedra Forest(Caddell Farm) → Acasa Marshes(Jova) | APC 키 → Acasa, Vitalis 집 열쇠 |
| 2 | Quest02 | Family Legacy | 1 | Vyssa Hills → Lake Cynon(Lymbic Forge) | Keystone of Grief, Blue Flower 대화 → Lake Cynon |
| 3 | Quest03 | Keystone of Terror | 2 | Talju → Marastan → Arcas Spire | Blood Queen's Sword, 타임루프 |
| 4 | Quest04 | Keystone of Ecstasy | 2 | Pathem Abbey → Lethe 도서관 → Vyssa 광산 → Plains of Mist | Land Grant |
| 5 | Quest05 | Keystone of Rage | 2 | Jeljin → Lethe 문화부 → Auriga Museum | Iron Seal 두 개 |
| 6 | Quest06 | Into the Unknown | 3 | Lake Cynon → Mount Obek(Eye of God) | 키스톤 4개, 오브 4개 — 엔딩 1개 |

- 데이터 번호와 공략 제목의 대응은 **퀘스트 아이템 이름으로 확인**: Quest02_KeystoneGrief·ForgeHammer, Quest03_TaljuParkingLotKey·
  ArcasSpireElevatorDoorKey, Quest04_LetheLibraryKey·VyssaHillsMinesKey, Quest05_KeystoneRage·AurigaMaintenanceKey·JeljinCraneKeys.
- **2막의 키스톤 3개는 순서 자유**(공략 권장 공포 → 분노 → 환희). 첫 키스톤을 얻으면 시간이 흘러 일부 선행 의뢰가 실패.
- 출처: https://hellisus.wiki.fextralife.com/Walkthrough · https://game8.co/games/Hell-is-Us/archives/543745 ·
  https://guided.news/en/guides/hell-is-us-walkthrough-the-complete-solution-to-all-3-acts/ · https://gamerant.com/best-order-to-get-keystones-hell-is-us/

## 3. 설계 제안

1. **진행 단계 계산** — 메인 퀘스트 = `QuestNN_DA`, NN 순. 현재 = 완료 상태 사실을 모르는 가장 낮은 번호(시작 사실을 알면
   "진행 중", 모르면 "다음"). 2막(03–05)은 순서 자유 → 시작된 것 중 진척(알게 된 사실 비율)이 높은 것, 없으면 03.
2. **목표 순위** — 기존 목표(모르는 사실을 주는 곳) 중 **현재 메인 퀘스트의 사실**을 주는 곳을 최우선. 그 안에서:
   ① 추정은 알고 확인을 모르는 트랙의 확인 사실(Priority 1) → ② 새 트랙의 첫 사실(Priority 0) → ③ 나머지. 같은 순위면 거리.
3. **범위 밖일 때** — 현재 퀘스트의 목표가 불러온 범위에 없으면(다른 지역): 위 표의 "주 지역"을 다음 갈 곳으로 안내 문구에 표시
   (지역 이름만 — 공략 문장은 옮기지 않음). 지역 좌표는 APC 목적지·지역 진입 시점에 모아 둘 수 있음(후속).
4. **패널** — 안내 카드에 "메인 스토리 3/6 · Keystone of Terror — 알아낸 단서 12/18", 다음 트랙 2–3개. 안내 모드에
   **"메인 스토리 자동"**(기존 "자동 — 가장 가까운 퀘스트 목표" 를 대체·확장).
5. **표시 이름** — `Quest0N_Name_DA`(StringFact)의 게임 내 문자열을 읽어 현지화된 이름을 씀(가능하면), 실패하면 번호.

한계: 사실을 주는 것이 NPC 대화일 때는 장소 오브젝트가 아니어서 목표로 못 찍힘 — 그 경우 "누구와 대화" 를 트랙 이름(인물)으로
표시하는 데 그침. 2막 순서는 플레이어 선택.

## 4. 구현 — 퀘스트 저널·추적기 (2026-10-03)

요청: "화면 우측 중앙에 현재 진행중인 퀘스트를 실제 이름·설명으로 오버레이, 패널에서 현행 퀘스트 선택 (MMORPG 방식)" +
"메인 퀘스트뿐 아니라 서브퀘스트도".

### FText 읽기 (build 24045435 실측)
- `FText` = TextData 포인터 + 플래그. TextData: +0 vtable, +8 참조 수, **+0x10 history**.
- **history +0x30 → +0x08 = FString — 현지화된 표시 문자열** ("가족 재회", "부모에 대한 단서를 찾아 조사하기").
  없으면 history +0x20 → +0x10 = FString — 원문(영어 "Family Reunion").
- `quests::ftext(m, at)` — 모든 TextProperty 에 그대로 씀 (StringFactData/LinkFactData.Description +0x60, SecretRow.Title …).

### 서브퀘스트 = 선행(Good Deeds)
- 게임에는 메인(Quest01–06) 말고 퀘스트 목록이 **선행·미스터리·타임루프**(데이터패드의 탐험 탭)뿐 — 서브퀘스트는 선행.
- 세이브: `CharlieSaveGame.Player.SecretsState.GoodDeeds` = `CharlieSecretEntryState`{Identity: Guid, bIsNew, State}
  (이 세이브 26개, State 1 = 진행 3개). State: 0 모름 · 1 시작 · 2 완료 · 3 보상 · 4 실패(추정, 1 만 실측).
- 정의: **`SecretsSubsystem.GoodDeeds`** (WorldSubsystem, 항상 로드) — `GoodDeedData` 0x70 바이트: Guid +0x18,
  StartedTag +0x28 (`Secrets.Facts.<Name>Started`), CompletedTag +0x30, `Title` FText +0x38. (`GoodDeeds_DT` 는
  탐험 탭을 열어도 메모리에 없었음 — 데이터 테이블은 서브시스템이 읽고 놓는 것.)
- 제목: 게임이 한 번 보여 준 것만 현지화 문자열이 붙음(history +0x30). 나머지는 **문자열 테이블 참조**:
  history +0x10 = TableId FName (`/Game/GameData/Secrets/UI_Secrets_ST`), +0x18 = 텍스트 키 풀 인덱스(u32).
  `UStringTable` +0x28 → FStringTable, +0x20 KeysToEntries (원소 32 B: 키 인덱스 u32, 엔트리 포인터 +8),
  엔트리 +0x10 = 원문 FString(영어). → 한국어가 있으면 한국어, 없으면 영어 원문. 한국어는 `quests.txt` 에 기억하고
  영어로 덮어쓰지 않음. 실측: 26개 모두 이름, 진행 3개는 「금시계」「이산가족」「빅터 그리고 비질」.

### 메인 퀘스트
- `QuestData` 객체 + `AssociatedQuestData` 로 묶인 사실들(트랙·텍스트). 이름 = `Name` 트랙 사실의 Description,
  설명 = `Desc…` 트랙. 상태: `Quest Status` 트랙의 Complete/Start 사실을 앎 → 완료/진행, 사실 하나라도 앎 → 진행.
- **단서(lead)** = 시작했지만 다 알지 못한 트랙(+ 마지막으로 알게 된 사실의 설명). 실측: Quest01 24/30, 단서 "Sabinian Officer".
- 이름·설명은 에셋이 스트리밍 아웃돼도 쓰도록 `quests.txt` 에 같이 기억.

### 비용
- GUObjectArray 전체(~35만)를 **4,000개씩** 엔진 스텝마다 훑음(클래스별 역할 캐시) — 한 패스 ~90스텝. 첫 스텝들은
  클래스 이름 조회로 ~100 ms, 이후 짧음. 저널은 매 스텝 지식(KnownFacts)·선행 상태와 대조.

### 따라가기·안내
- 선행과 목표 연결: 목표의 새 태그가 선행의 태그 접두(`Secrets.Facts.GoldenWatch`)로 시작하면 그 선행을 진행시킴.
- `MapState.quest`: 패널에서 고른 퀘스트 키(`Quest01` 또는 선행 GUID). 없으면 **메인 스토리 자동** = 진행 중인 가장
  낮은 번호(없으면 시작 전 첫 번호). 고른 퀘스트가 끝나면 자동으로 돌아감.
- 자동 안내: 따라가는 퀘스트를 진행시키는 목표(목표의 새 사실 `AssociatedQuestData` 가 그 퀘스트, 또는 새 태그가
  선행 태그 접두로 시작) 중 가장 가까운 곳 → 없으면 가장 가까운 퀘스트 목표.
- 추적기(오버레이, `ui/tracker.rs`): 게임 창 오른쪽 가운데, 340×≤560. 따라가는 퀘스트는 펼침(이름·종류·단서 수·설명
  4줄·단서 3개), 나머지 진행 중 5개는 한 줄. 글자는 GDI(맑은 고딕, 그레이스케일 AA)로 DIB 에 그려 밝기를 커버리지로
  합성(`ui/pen.rs`) — 래스터의 획 글꼴은 숫자·방위만 앎. 내용이 바뀔 때만 다시 그림.

## 5. 퀘스트 따라가기 = 경로 안내 + NPC·아이템·전달 목표 (2026-10-03)

요청: "퀘스트 추적 시 경로찾기가 활성화되고 갱신되어야" + "필수 NPC, 아이템, 단서 등 경로안내".

### 따라가기
- 패널에서 퀘스트를 고르면 `route = true`, 자동 안내 켜짐, 목표 초기화. `MapState.chosen`(저장 안 함) = 목록·F11 로 손으로
  고른 목표 → 다 쓰일 때까지 유지. 아니면 매 프레임: 지금 목표가 따라가는 퀘스트에 맞지 않으면(퀘스트를 바꿨거나 그
  퀘스트의 목표가 범위에 들어옴) 내려놓고, 가장 가까운 맞는 목표로. 목표가 쓰이면 다음 것으로 → 경로도 따라 갱신.
- 맞는 목표(`wanted`): 따라가는 퀘스트를 진행시키는 것이 하나라도 로드돼 있으면 그것만. 없으면 메인 퀘스트는 아무
  퀘스트 목표, 선행은 없음(추적기에 "이 지역엔 … 목표가 없음").

### NPC (대화)
- `NpcActor` 의 `FlowComponent.RootFlow`(+0x198) = FlowAsset. `FlowAsset.Nodes`(+0x50) TMap<FGuid, UFlowNode*>, 원소 32 B,
  노드 +0x10. `FlowNode_Payload` +0x1D0 = PayloadData(ContainedFacts set +0x18, TagFacts +0x68, ItemsToAdd +0x88).
- 주제(`FlowNode_TopicSubGraph` TopicAsset +0x218, `FlowNode_SubGraph` Asset +0x1D0)는 소프트 참조 — FSoftObjectPath 의
  AssetName FName 이 소프트 포인터 +0x10. 로드된 FlowAsset 을 이름으로 찾음(quests.rs 패스가 모아 둠), 2단계까지.
- 실측: `VictorGaz_ConvoTopic_*`, `DetainedPriest_ConvoTopic_*` 페이로드가 Quest01 사실(예: `SabinianOfficer_FamilyReunion_
  Quest_TextFact_DA`)과 아이템(`Quest01_FamilyHomeKey_item_DA`)을 줌. `Conversation.TopicsUnlock.*` 태그만으로는 목표 아님.
- NPC 는 주제가 스트리밍되므로 스캔마다 다시 읽음.

### 전달 (선행의 아이템 건네기)
- `TradeGiveItemRuneComponent.Rune.ValidTrades` = `DonationRequest`{Item +0, TradeReplies, Payload +0x18 …}. 보상 페이로드의
  태그(예: `Secrets.Facts.<Deed>Completed`)로 선행과 연결 → "전달: <아이템>".

### 아이템
- 상호작용 오브젝트 페이로드의 `ItemsToAdd` 중 `/Items/Quests/`·`/Items/Secrets/` 아이템 → 가져가기 전까지 목표.
  `QuestNN_` 로 시작하면 그 메인 퀘스트(`Goal.keys`). 실측: `Quest03 PholGuardGeneralScroll`, `SenedraCaddellsTreasureNote02`.

### 한계 (실측)
- 금시계(Caddell_GoldenWatch)는 이미 인벤토리에 있고, 받는 NPC 는 이 지역(Senedra)에 로드되지 않음 → 역참조 스캔에서도
  선행 태그·아이템을 가리키는 월드 오브젝트 없음. 그 NPC 가 있는 지역에 가면 전달 목표가 잡힘.
- `GoodDeedData.LocationNameFact` 는 출시 데이터에서 비어 있음(26개 모두 null) — 선행 장소 표시 불가.
