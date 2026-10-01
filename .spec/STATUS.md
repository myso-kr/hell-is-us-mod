# 현재 상태 (2026-10-02)

## 한 줄 요약

dungeons2-mod 의 외부 프로세스 구조를 Hell Is Us(Steam, UE 5.5)로 옮겼다. Steam 빌드
24045435 에서 **`doctor` 전 항목 ok**. 앵커 탐색, 리플렉션 체인, 주인공 게이트, 위치,
속성 8세트 69개, 치트 표가 쓰는 속성 17개가 모두 실제 게임에서 해석됐다
(2026-10-02, 첫 시도에 통과). 플레이 테스트 두 번 끝에 **치트 4개 확인** (`god`, `stamina`,
`speed`, `hero_time`). 계수형 속성 11개는 효과가 없어 뺐다 — 바깥에서는 안 되는 종류 (D12).

## 저장소

- 로컬: `%USERPROFILE%\hell-is-us-mod`
- GitHub: 아직 없음. 만들 때는 `myso-kr/hell-is-us-mod`, **비공개**로 (공개는 사용자가 결정).
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
- 미니맵: 다른 지역으로 이동 시 경로가 따로 쌓이는지, DPI 배율에서 위치. (F6 은 문제없음. F7 은 게임 사진 모드와 겹쳐 → 기본값을 F9 로 바꾸고 키를 고를 수 있게 함, F9 확인됨 2026-10-02)

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
