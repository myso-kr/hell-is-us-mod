# 현재 상태 (2026-10-02)

## 한 줄 요약

dungeons2-mod 의 외부 프로세스 구조를 Hell Is Us(Steam, UE 5.5)로 옮겼다. Steam 빌드
24045435 에서 **`doctor` 전 항목 ok**. 앵커 탐색, 리플렉션 체인, 주인공 게이트, 위치,
속성 8세트 69개, 치트 표가 쓰는 속성 17개가 모두 실제 게임에서 해석됐다
(2026-10-02, 첫 시도에 통과). 플레이 테스트 두 번 끝에 **치트 4개 확인** (`god`, `stamina`,
`speed`, `hero_time`). 계수형 속성 11개는 효과가 없어 뺐다 — 바깥에서는 안 되는 종류 (D12).

## 저장소

- 로컬: `%USERPROFILE%\hell-is-us-mod` (원격: github.com/myso-kr/hell-is-us-mod, 공개 예정)
- GitHub: 아직 없음. `myso-kr/hell-is-us-mod`, **공개** 예정 (사용자 결정, 2026-10-03). 커밋 신원은 GitHub noreply
  (`myso-kr <1237913+myso-kr@users.noreply.github.com>`) — 기록 전체를 다시 써 실명·이메일·로컬 경로를 지움.
- CI: `.github/workflows/ci.yml` (dungeons2-mod 와 같음: fmt, clippy -D warnings, test,
  게임 파일 커밋 검사). 원격이 없어 아직 돌지 않음.

## 된 것 (코드, 게임 없이 테스트됨)

| 영역 | 상태 |
|---|---|
| 설치 찾기 | 레지스트리 SteamPath → `libraryfolders.vdf` → `appmanifest_1620730.acf` 의 `installdir`·`buildid`. 실제 PC에서 동작 확인 |
| 실행 | `steam://rungameid/1620730` (explorer 경유) |
| 앵커 | **고정 오프셋 없음.** 붙을 때마다 FNamePool 과 GEngine 을 이미지의 쓰기 가능 섹션에서 찾고, FField 레이아웃을 후보 4개 중 `GameEngine.GameInstance` 가 읽히는 것으로 고른다 (`anchors.rs`) |
| 플레이어 체인 | 리플렉션으로 이름을 따라가 오프셋을 배움: GameInstance → LocalPlayers[0] → PlayerController → Pawn → (AbilitySystemComponent 를 가리키는 속성) → SpawnedAttributes. 위치: RootComponent.RelativeLocation, ControlRotation (`player.rs`) |
| 주인공 게이트 | 조작 중인 폰이 `CharlieCharacterHero` 계열일 때만 쓰기. 로딩·메뉴·시네마틱에서는 닫히고, 닫혀 있는 동안 토글은 **일시 정지**(끄지 않음) |
| 속성 | 세트 이름 `*` = 이름이 유일한 세트에서 찾기. 둘 이상이면 거부 (`attr.rs`) |
| 치트 표 | 7행. 확인 4 (`god`, `stamina`, `speed`, `hero_time`), 미시도 3 (`lymbic`, `lymbic_cost`, `skill_cooldown`) |
| CLI | `doctor`, `list`, `get`, `set`, `hold`, `restore`, `pose`, `ui` |
| 패널 | dungeons2-mod 의 F8 오버레이 그대로 (탭: 생존·전투·이동·지도·디버그). 스냅숏 10 Hz |
| 무기 경험치 배수 | **게임에서 동작 확인 (2026-10-03, ×3: 265 → 580).** doctor watch 로 찾은 네이티브 필드(+0x13C 누적, +0x140 레벨 안, +0x148 다음, +0x130 레벨, +0x138 상한). .spec/DOCTOR.md 사례 |
| doctor 확장 | **실제 데이터로 확인 (2026-10-03).** inspect · find · dump · watch · scan(next). UE4SS 없이 반사 + 네이티브 빈 구간·스캔. .spec/DOCTOR.md |
| 치트 A 단계 | **모두 게임에서 동작 확인 (2026-10-03).** 게임 속도, 적 속도, 약한 적, 소모품 줄지 않음, 샤드 수량, 위치 슬롯 5개. 새 '아이템' 탭. extras.rs. .spec/CHEATS-RESEARCH.md §4.1 |
| 윤곽선 스타일 · 표시 순환 | **실제 데이터로 렌더 확인, 게임 화면 미확인 (2026-10-02).** 큰 지도 기본 윤곽선(투명 배경, 벽·물가·등고선 선만). 지도 키 하나로 미니맵 → 큰 지도 → 끔 순환, 큰 지도 키 없앰. 패널 그리드 레이아웃. .spec/GUIDE.md §12–13 |
| 렌더 성능 · 콘솔 패널 | **벤치로 확인, 게임 화면 미확인 (2026-10-02).** 큰 지도 전부 켬 261 → 27 ms, 미니맵 38 → 6 ms. 큰 지도 불투명도. 패널 960 px 사이드바 + 2열, 지도·안내·북쪽 보정 통합. .spec/GUIDE.md §10–11 |
| 지형 음영·등고선 | **실제 데이터로 렌더 확인, 게임 화면 미확인 (2026-10-02).** 지도 탭 '지형 표시' 끔/음영/등고선/둘 다. 높이맵을 별도 스레드에서 굽고(540² 31 ms) 픽셀마다 등고선. .spec/GUIDE.md §9 |
| 물·경사·다리 | **실제 데이터로 확인, 게임 화면 미확인 (2026-10-02).** 죽는 물 상자 + Chaos 지형 높이맵(256개)으로 물 1,104 구간, 경사 비용, 칸별 걷는 면(다리 데크). 경로 계산은 별도 스레드. .spec/GUIDE.md §8, D15 |
| 장애물 경로 | **실제 데이터로 렌더 확인, 게임 화면 미확인 (2026-10-02).** 경로가 지형을 무시한다는 피드백 → 게임 지상 NavMesh 는 메모리에 없음(14 GB 검색) → 메시 충돌 형상(BodySetup.AggGeom)과 인스턴스·ComponentToWorld 로 장애물 69,231개, 주인공 높이에서 완전 차단. 바위 띠 틈으로 돌아감 확인. 지형 경사·물은 아직 모름. .spec/GUIDE.md §7 |
| A* 경로·큰 지도 | **코드 완료, 실제 데이터로 렌더 확인(경로 13점·360 m·22 ms), 게임 화면 미확인 (2026-10-02).** 벽을 돌아가는 경로를 미니맵·큰 지도에, 나침반은 경로 방향·경로 거리. 큰 지도 F3(화면 가운데 80%). 게임 메뉴(커서 표시 또는 일시정지)면 모든 오버레이 숨김 — 실제 인벤토리에서 신호가 오는지 미확인. 상세 .spec/GUIDE.md §6 |
| 나침반·안내 | **코드 완료, 실제 데이터로 렌더 확인, 게임 화면 미확인 (2026-10-02).** 화면 위 가운데 나침반(F10), 안내 대상 순환(F11), 패널 '안내' 탭. 목표 = 아직 모르는 사실·태그를 주는 상호작용 오브젝트(저장 상태와 페이로드 비교) → 퀘스트/비밀/단서. 테스트 지역 11곳. 상세 .spec/GUIDE.md |
| 저장 지점 그룹 | **사용자 요청으로 추가 (2026-10-02), 화면 미확인.** 수동 저장 오브젝트 = 계보에 `InteractableCheckpointActor` (`Base_SavePoint_Interact_BP_C`). 테스트 지역에 6개. 자동 저장 트리거(`OverlapCheckpointActor`)는 보이지 않는 영역이라 제외. 노란 등불 아이콘. 저장 지점은 여러 번 쓰므로 '사용됨' 숨김에서 제외. 예전 minimap.txt(`layers_version` 없음)는 새 그룹을 켠 채로 읽음 |
| 미니맵 3단계 | **코드 완료, 게임 화면 미확인 (2026-10-02).** 게임 월드맵은 국가 지도라 부적합(.spec/MAP.md) → 사용자 결정 B: 로드된 정적 메시(테스트 지역 8,100개)를 위에서 본 윤곽으로 그려 배경으로. 주인공 층 높이만, 바닥/벽 두 겹, 겹침은 합집합 + 윤곽선. 지도 탭 '벽·바닥 윤곽' 스위치. **게임에서 확인: 전체적으로 좋음.** 피드백(큰 지형이 안 보임, 높낮이 구분 어려움) → 발 기준 높이 6개 층(깊은 곳·낮은 곳·같은 높이·높은 곳·절벽·바위·벽) + 층마다 윤곽선(등고선 역할), 범례 — 화면 미확인 |
| 미니맵 2단계 | **게임에서 확인: 위치·회전 정상 (2026-10-02).** 이어서 고친 것 두 가지(화면 미확인): ① 다 쓴 것 숨김 — 적은 `HealthAttributeSet.Health` ≤ 0(시체), 아이템·전리품·문은 `InteractionActionComponent.bHasBeenActivated` 가 켜지면(주움/사용). 액터마다 위치를 한 번 찾아 캐시. 테스트 세이브에서 119개 → 59개. ② 점 대신 SVG 아이콘 — `assets/icons/*.svg` 5종(해골·다이아몬드·상자·사람·문)을 resvg 로 시작할 때 16px 로 래스터화. 실패하면 점으로 그림. 처음 구현: 로드된 모든 레벨의 액터를 1초마다 훑어 클래스 계보로 분류: 적(`CharlieLymbicEntity`), 아이템(`Base_Item_GatherSingleUse_Interact_BP_C`), 전리품(`Base_EnemyLootContainer_BP_C`), NPC(`NpcActor`), 문·퍼즐(`InteractableDoorActor`, `LymbicLockPanel*`, `*DroneTranslation*`). 위치는 0.1초마다. `doctor` 실측: 레벨 140개·액터 10,671개 중 119개, 스캔 18 ms (release). 지도 탭에서 종류별로 켜고 끔 |
| 미니맵 | **게임에서 확인 (2026-10-02): 표시·회전 정상, F6 마커, 경로 기록, 지역 이름 `SenedraForest_Root_WP`.** 클릭 통과 레이어드 Win32 창, 게임 창 오른쪽 위 240px. 지나온 길(3 m 간격, 50 m 넘게 튀면 끊음, 지역당 6000점), 마커(F6, 5 m 안에서 다시 누르면 삭제), 주인공 화살표, N 표시. 진행 방향 위/북쪽 위, 반경 20–300 m. 표시/숨김 F9 (기본값, 패널에서 F1–F12 중 변경 가능 — F7 은 게임 사진 모드라 피함). 지역(월드 이름)별로 `Mods\minimap.txt` 에 10초마다 저장 |

## 게임에서 확인한 것 (빌드 24045435, 2026-10-02 `doctor`·`list`)

| 항목 | 결과 |
|---|---|
| 이름 풀 | +0x9220CC0 (유일) |
| GEngine | +0x947CF10 (유일) |
| FField 레이아웃 | 후보 2번: next 0x18, name 0x20, size 0x34, **offset 0x44** (UE 5.6 은 0x48) |
| 주인공 계보 | StoryHero_BP_C < CharlieCharacterHero < CharlieCharacterGAS < CharlieCharacter < Character < Pawn < Actor < Object |
| 체인 | GameInstance +0x11F8, LocalPlayers +0x38, PlayerController +0x30, Pawn +0x2E8, ASC +0x688, SpawnedAttributes +0x1088 (CE 테이블과 일치) |
| 위치 | `pose` — 걸으면 x/y/z 가 움직이고 yaw 가 바뀜 (확인) |
| 속성 | 8세트 69개: Endurance 13, Lymbic 10, Damage 6, Weapon 7, Locomotion 3, Poise 5, PlayerDefense 7, Player 18 |

`list` 로 본 기본값에서 알게 된 것 (치트 표에 반영함):
- 기본값이 0 인 `*Coefficient` 는 더해지는 보너스다 (`MovementSpeedModifierCoefficient`,
  `MeleeDamageBoostCoefficient`, `GlobalEnduranceCostModifierCoefficient`,
  `DamageTakenBoostCoefficient`). 기본값이 1 인 것은 배수다.
- `PoiseAttributeSet` 은 주인공에게 전부 0 이라 쓰이지 않는다 → `poise` 치트 삭제.
- `GlobalDefenseCoefficient` 는 0.3 (base 0, 장비 효과)으로 피해 감소율로 보인다 → 범위 0–0.95.
- `EnduranceMax` 5000 (base 4000), `EnduranceCap` 5000, `LymbicEnergyMax` 700.
- `CurrentMaxWalkingSpeed` 315 (base 450) 는 이동 속도의 실제 결과값이다.

## 첫 플레이 테스트 (2026-10-02, 사용자 verify.txt)

| 결과 | 치트 |
|---|---|
| 됨 | `god` (EnduranceCap ← EnduranceMax), `stamina` (Endurance ← EnduranceCap) |
| 안 됨 → 교체 | `speed` (MovementSpeedModifierCoefficient), `attack_speed` (MeleeAttackAnimPlayRateMultiplier), `dodge_speed` (DodgeAnimPlayRateMultiplier) |
| 안 됨 → 삭제 | `stamina_cost` (EnduranceCostCoefficient), `damage` (MeleeDamageBoostCoefficient) |
| 기록 없음 | `healing`, `defense`, `iframes`, `lymbic`, `lymbic_cost`, `skill_cooldown`, `drone_cooldown`, `xp_gain` |

왜 안 됐나 (추정, DECISIONS D12): 게임이 매번 직접 읽는 속성(Endurance 계열)은 쓰면
바로 먹지만, 계수형 속성은 게임이 GAS 집계기(FAggregator, 자기 BaseValue 를 따로 가짐)나
GameplayEffect 계산 시점의 캡처로 읽는다 — 속성 세트 메모리를 바꿔도 그 경로에는 안 닿는다.

대응: 읽기 전용 probe 로 주인공과 이동 컴포넌트의 속성을 덤프해
- `HeroMovementComponent.MaxWalkSpeed` +0x268 (450) — CE 테이블의 질주 속도 쓰기 위치와 같음
- `StoryHero_BP_C.CustomTimeDilation` +0x68 (1.0)
을 찾아 `speed`, `hero_time` 으로 교체. 근접 피해는 `WeaponAttributeSet.WeaponAttackPower`
(238) 로 실험 (`weapon_power`).

## 두 번째 플레이 테스트 (2026-10-02)

| 결과 | 치트 |
|---|---|
| 됨 | `speed` (이동 컴포넌트 MaxWalkSpeed), `hero_time` (주인공 CustomTimeDilation) |
| 안 됨 → 삭제 | `healing`, `defense`, `iframes`, `drone_cooldown`, `xp_gain` (모두 *Coefficient), `weapon_power` (WeaponAttackPower) |

결론: 바깥 프로세스로 되는 것은 게임이 직접 읽는 현재 자원(Endurance, LymbicEnergy)과
일반 필드(이동 컴포넌트, 액터)뿐이다. 계수·무기 수치는 GameplayEffect 계산 경로라 안 된다
— 피해·방어·쿨다운·경험치 치트는 Phase 1 (게임 안에서 GameplayEffect 적용) 몫.

## 확인 못 한 것

- `lymbic`, `lymbic_cost`, `skill_cooldown` (미시도 — 림빅은 게임 초반이라 아직 열리지 않은 기능. 뒤의 둘은 계수형이라 실패 예상).
- 메인 메뉴·로딩 중 게이트가 닫히고 다시 열리면 토글이 재개되는지.
- 미니맵 2단계: 주운 아이템·시체 사라짐 **확인**, 아이콘 크기 적당 **확인** (2026-10-02). 이어서 사용자 요청으로
  아이콘 크기 설정(10–32px, 지도 탭)과 세부 종류 필터(22종, 그룹을 펼쳐 개별 끄기)를 추가 — 바로 적용됨 **확인**, 분류 문제없음.
  사용자 피드백(그룹 토글이 잘 안 보임, 설정창이 오밀조밀, 숫자가 헷갈림)으로 패널 UX 개편 — 화면 미확인:
  패널 폭 500, 간격·글자 키움, 지도 탭을 네 상자(미니맵·단축키·표시할 것·이 지역)로, 그룹은 스위치 + 종류 색 점,
  세부는 '세부 ▼' 로 펼치는 색 칩, 개수는 괄호. **확인: 안정적, 다만 여백·글자가 컸음** → 폭 470, 본문 14,
  간격 8×6, 버튼 높이 23 으로 한 단계 줄임 — **적당함 확인**.
- 미니맵: (보류 — 테스트 세이브가 아직 다른 지역으로 갈 시점이 아님, 2026-10-02) 다른 지역으로 이동 시 경로가 따로 쌓이는지, DPI 배율에서 위치. (F6 은 문제없음. F7 은 게임 사진 모드와 겹쳐 → 기본값을 F9 로 바꾸고 키를 고를 수 있게 함, F9 확인됨 2026-10-02)

## 알려진 한계

- **Steam 업적을 막지 않음.** 치트 중 달성한 업적도 Steam 에 올라간다.
- 독점 전체 화면에서는 패널이 안 보임. 창 모드/테두리 없는 창 모드 필요.
- 미니맵에 지도 그림은 없음 — 지나온 길과 마커만 (ROADMAP 3 의 3단계).
- 재화·인벤토리 수량은 GAS 속성이 아니라 다루지 않음 (ROADMAP 4).

## 사용자 환경에 남아 있는 것

- 게임 폴더에 이 도구가 만든 것은 없음. 처음 실행하면
  `C:\Program Files (x86)\Steam\steamapps\common\Hell Is Us\Mods\` 를 만든다
  (쓰기 불가면 `%LOCALAPPDATA%\hiumod\`).
- UE4SS 등 다른 모드는 설치하지 않았음.
