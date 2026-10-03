# 치트 확장 조사 (2026-10-03)

요청: "웹리서치를 통해 더 다양한 치트 구현을 위한 조사 (필요하면 UE 를 이용한 doctor 확장도 고려)."
두 갈래로 조사했다 — 기존 트레이너·치트 테이블(웹), 그리고 게임 메모리 직접 프로브(읽기만, build 24045435).

## 1. 웹: 기존 트레이너가 주는 것

FearlessRevolution 은 직접 접근이 막혀 있어 Wayback Machine 사본으로 스레드와 .CT 파일 7개를 읽었다
(Tuuuup! v2 = Steam 1.5.40 / exe 5.5.4.0, ndck1, VampTY, Sianz, matthew80, DarkMango). 오프셋은 패치마다 바뀌지만 이름은 같다.

| 기능 | 제공 | 구현 힌트 |
|---|---|---|
| 체력 무한 / 갓 모드 | WeMod, CH, PLITCH, DarkMango, Tuuuup! | **HP = `EnduranceCap`**, 스태미나 = `Endurance` (우리 구현과 같음). Tuuuup! 는 속성 갱신 함수를 훅해 플레이어 세트만 감소를 0 으로 |
| 원킬 / 쉬운 처치 | WeMod, PLITCH, DarkMango, Tuuuup! | 적(비플레이어)의 Endurance 를 10 으로 |
| 무한 아이템 / 무료 사용 | WeMod, CH, PLITCH, DarkMango | 아이템 +0x48 수량. 코드 쪽은 수량 빼기 AOB `41 2B C1 45 3B C8`. 정의 +0xD8 `QuantityMax`, +0xDC `bCanMultiStack` |
| 선택 아이템 수량 지정 | WeMod, PLITCH, Tuuuup!, ndck1 | 인벤토리에서 살펴본 아이템을 잡음 |
| 샤드 무한 / 제작 시 소모 없음 | CH, Tuuuup! | 샤드는 보통 스택 아이템(같은 수량 칸) |
| 줍기 배수 | Tuuuup! | 추가/제거 함수에서 양수 변화량을 곱함 (코드 훅) |
| 무기 경험치 배수 | CH, PLITCH, Tuuuup! | AOB `41 03 8E 3C 01 00 00` — 무기 인스턴스 +0x13C XP, +0x148 최대 XP |
| 무기 등급·모듈 칸 | Tuuuup!, ndck1 | 정의 +0x1A0 `Grade`, +0x1C8 `MaxModuleSlots` |
| 피해·방어 배수 | CH (피해 배수), 요청 다수 | `MeleeDamageBoostCoefficient`, `WeaponAttackPower`, `GlobalDefenseCoefficient`, `DamageTakenBoostCoefficient` … |
| 적 속도 / 적 정지 | CH, PLITCH, DarkMango, Tuuuup! | 비플레이어 폰의 `CustomTimeDilation` |
| 게임 속도 | 전부 | 전역 시간 배속 (WorldSettings `TimeDilation`) |
| 날기 / 노클립 | CH, PLITCH | `CharacterMovement.MovementMode`, 충돌 끄기 |
| 위치 저장·복원 (5칸) | CH | 액터 위치 |
| 패리·무적 시간, 애니 배속 | 요청 / VampTY 속성 목록 | `InvincibilityWindowModifierCoefficient`, `*AnimPlayRateMultiplier` |
| 드론 쿨다운 | matthew80, Tuuuup! | `DroneDockedAbilityCooldownModifierCoefficient` (Tuuuup! 는 [rbx+0x110]+0x40 float 를 씀 — 구조 미상) |

출처: [WeMod](https://community.wemod.com/t/hell-is-us-cheats-and-trainer-for-steam/370869) ·
[MrAntiFun](https://mrantifun.net/threads/hell-is-us-trainer.26160/) · [Cheat Happens](https://www.cheathappens.com/81830-PC-Hell-is-Us-trainer) ·
[PLITCH](https://www.plitch.com/en/games/hell-is-us-385402285381128192) · [FLiNG (16 옵션, 이름 미확인)](https://www.fling-trainer.com/hell-is-us-trainer/) ·
FearlessRevolution [Tuuuup!](https://fearlessrevolution.com/viewtopic.php?t=36512), [ndck1](https://fearlessrevolution.com/viewtopic.php?t=36338),
[데모 스레드](https://fearlessrevolution.com/viewtopic.php?t=35282), [DarkMango](https://fearlessrevolution.com/viewtopic.php?f=5&t=36430) (web.archive.org 경유).

게임 메커니즘 (Fextralife, Game8): 샤드 = 재화이자 제작 재료(Neutral/Ecstasy/Grief/Rage/Terror × 소·중·대), 무기 등급 1–5
(2등급부터 글리프 3칸), 방어구 4등급까지, 소모품 = 의약품(즉시/지속, 소지 한도)·림빅 축전지/배터리·억제기·업그레이드 책, 드론 스킬 4칸.

## 2. 메모리 프로브 (읽기만, 이 빌드에서 확인)

- **속성 세트 8개** (ASC `SpawnedAttributes` 순서): Endurance, Lymbic, Damage, Weapon, Locomotion, Poise, PlayerDefense, Player.
  실측 예: `EnduranceCap 5000 / EnduranceMax 5000`, `LymbicEnergy 554/554`, `WeaponAttackPower 299`,
  `CurrentMaxWalkingSpeed 450`, `AbilityCooldownModifierCoefficient 0.1`(쿨다운 치트 켜짐 상태), `DefensivePower 0/50`.
- **인벤토리**: `CharlieInventoryLoadoutSubsystem` → `CharlieInventory`(Owner = 주인공) → `Items` TArray (53개). 아이템 클래스:
  `CharlieInventory{Item, UseableItem, UseableCooldownReductionItem, UseableShowImageItem, WeaponItem, GearItem, ShardItem,
  DroneItem, DroneModuleItem, CosmeticItem, DroneCosmeticItem, LevelSequenceItem}`. 반사된 것은 `ItemData`(+0x40, 데이터 에셋)뿐.
- **스택 수량 = 아이템 +0x48 (u32, 네이티브)** — 세이브의 `Quantity` 와 일치: `Cons_MedicineCivilianT01` 9,
  `Craft_Shards_Neutral_G01` 122, `…Rage_G01` 75, `…Terror_G01` 33, `…Ecstasy_G01` 18, `Cons_DroneBoosterT01` 6, `Cons_LymbicChargerT01` 6.
- 세이브 `CharlieSaveGame.Player.Inventory.Items` 원소(176 B): Guid, ItemData(soft), **Quantity @+0x38**, ItemPlacementSlotIdx,
  BoundModules, **WeaponCurrentXP @+0x90**, bIsNew, DroneCurrentCosmeticItemID, ItemPreferenceState. (세이브 시점의 사본 — 라이브가 진짜)
- 주인공 소유: `CharlieHeroAbilitySystemComponent`, `DamageOverTimeComponent`, `ParryComponent`, `HeroMovementComponent`, 능력
  (`StoryHero_*_GA_C`, `Drone_*_GA_C`, 기어 `DefensiveGear_*_GA_C`) 등.

## 3. 핵심 제약 — 왜 계수 치트가 안 됐나 (D12)

플레이 검증(verify.txt, 2026-10-02): 게임이 **직접 읽는 값**(Endurance, 이동 컴포넌트, 시간 배속)은 메모리 쓰기로 동작.
GAS 가 **다시 계산하는 값**(`*Coefficient`, 무기 공격력)은 메모리에 유지돼도 효과가 없었다 — 수식이 집계기(aggregator)의
기본값+모디파이어로 다시 만들어지거나, 능력이 시작 시점에 값을 캡처하기 때문. 테이블들이 데이터 대신 **코드 훅(AOB)** 을 쓰는 이유.

## 4. 구현 단계 제안

### A. 지금 방식(바깥 프로세스, 데이터 쓰기)으로 가능 — 위험 낮음
| 치트 | 방법 | 비고 |
|---|---|---|
| 소모품 무한 | `Useable*` 아이템 +0x48 수량을 켠 시점 값 이상으로 유지 | 정의 `QuantityMax` 로 상한 |
| 샤드 최대 / 지정 | `ShardItem` 수량 쓰기 (패널: 종류별 값) | 재화. Set 형(한 번 쓰기) |
| 원킬 (적 체력 1) | 적 액터(스캐너가 이미 앎) ASC 의 Endurance(·Cap) 를 낮게 | 적 HP 가 Endurance 인지 검증 필요 |
| 적 느리게 / 정지 | 적 폰 `CustomTimeDilation` (0.1–1) | 주인공 시간 배속과 같은 방식 — 검증된 경로 |
| 게임 속도 | WorldSettings `TimeDilation` | 전역 |
| 위치 저장·복원 / 지도 클릭 순간이동 | 루트 컴포넌트 위치 쓰기 | 물리·스트리밍 확인 필요 |
| 무기 경험치 | 무기 아이템 +0x13C XP (+0x148 최대) | 오프셋은 1.5.40 기준 — 반사·실측으로 재확인 |
| 날기 / 노클립 | `CharacterMovement.MovementMode` = Flying + 캡슐 충돌 끔 | 입력(상하) 처리 필요 |

### B. 코드 패치(AOB) — 테이블들이 쓰는 방식
수량 빼기 NOP(`41 2B C1 45 3B C8`), 무기 XP 배수(`41 03 8E 3C 01 00 00`), 속성 갱신 훅. `WriteProcessMemory` 로 코드 바이트를
바꾸는 것도 바깥 프로세스로 가능하지만 패치마다 패턴이 깨질 수 있고, 복원·검증 장치가 필요. A 로 안 되는 것만.

### C. UE 게임 안 모듈 — "doctor 확장"
- **UE4SS 는 이 게임에서 동작** (UE 5.5 사전 설정판, [Nexus 43](https://www.nexusmods.com/hellisus/mods/43)). 공개 SDK 덤프는 없고,
  반사는 난독화되지 않음.
- 쓸모: (1) **doctor 확장** — UE4SS 오브젝트 덤퍼·UHT 헤더 생성으로 네이티브 필드(수량 +0x48, XP +0x13C 등)를 이름으로 확인,
  Live View 로 검증. (2) **계수 치트의 정공법** — Lua 에서 ASC 의 `BP_ApplyGameplayEffectToSelf` 같은 UFunction 을 불러
  GameplayEffect 로 피해·방어·XP·쿨다운 배수를 걸면 GAS 가 스스로 계산하므로 D12 의 벽을 넘는다.
- 비용: D1(게임 파일 무수정) 변경, `dwmapi.dll` 프록시 설치, 크래시가 게임으로 번질 수 있음. 지금 패널과는 파일·명명 파이프로 연결.

## 4.1 A 단계 구현 (2026-10-03)
- `game_speed`(WorldSettings.TimeDilation, 기존 필드 방식 — 원래 값 디스크 기록), `enemy_time`·`frail`(extras.rs — 스캐너의 살아
  있는 적, 원래 값은 적마다 메모리에), `stock`·`shards`(인벤토리 — `Attached::inventory()` 가 GUObjectArray 에서 한 번 찾고
  매번 Owner=주인공 확인, 수량 = `ItemData`+8, `QuantityMax` 넘는 값은 레이아웃이 바뀐 것으로 보고 건너뜀), 위치 슬롯 5개.
- 적의 체력은 `HealthAttributeSet.Health/HealthMax` (Tier 1 680–1130) — 주인공의 Endurance 와 다른 세트.
- 소모품 줄지 않음: 0 이 되면 스택이 인벤토리에서 빠져 되돌릴 수 없으므로 최소 2 로 유지.
- 샤드: 가진 종류의 스택만(없는 종류는 만들지 못함), 999 상한.
- 모두 미검증 — 게임에서 확인 후 .spec/CHEATS.md 의 Seen in play 갱신.

## 5. 권고
1. **A 단계부터**: 소모품 무한, 샤드 지정, 적 느리게/정지, 게임 속도, 원킬(검증 후), 위치 저장·복원. 모두 이미 있는 반사·스캐너
   위에서 되고, 실패해도 값만 되돌리면 된다.
2. 피해·방어·XP 배수는 **C 단계(UE4SS + GameplayEffect)** 가 정공법. 원하면 doctor 확장(덤프·검증)부터 작은 단계로.
3. B(코드 패치)는 A·C 로 안 되는 것만, 패턴 검증·자동 복원과 함께.

## 6. 고스트 · 피격 무시 조사 (2026-10-03)

요청: "적에게 발견되지 않는 고스트, 피격판정 무시 God 를 추가할 수 있는지." doctor find/inspect 로 조사.

### 고스트 — 팀
- `CharlieCharacter.TeamID`(GenericTeamId, +0x67C) / `Faction`(+0x67D): **주인공 1/1, 적(HollowWalker 계열) 2/2**.
  `HazeGhost`, `CharlieActorGAS` 도 같은 필드. UE AI 지각은 감지 대상의 팀 관계(적대/중립/우호)로 거른다
  (`AISenseConfig_Sight/Hearing.DetectionByAffiliation`). 주인공을 적의 팀으로 두면 "아군"으로 넘길 가능성.
- 다른 갈래(나중): `HearingStimuliEmitterRuneComponent`·`FootstepEmitterRuneComponent`(소음), `CharlieCombatStateHandler.AggroedEntities`
  (이미 어그로된 적), `LymbicEntitySensesParameters.AggroRangeSightRatio`.
- 구현 `ghost`(실험): 켜면 주인공 TeamID/Faction 을 근처 적의 값(없으면 2/2)으로, 끄면 원래 값. 위험: 이미 싸우는 적은 계속
  싸울 수 있음, 같은 팀이라 내 공격이 안 먹힐 수 있음, 패널이 강제 종료되면 다시 불러올 때까지 남음.

### 피격 무시
- 이 게임의 무적은 **게임플레이 태그** 판정 (`DamageDealerThrowableComponent.bIgnoreInvincibilityTags`, `GameplayEffect.GrantedApplicationImmunityTags`).
  태그를 붙이려면 ASC 의 태그 카운트 맵·컨테이너에 원소를 넣어야 해 게임 안 메모리 할당이 필요 — 데이터 쓰기로는 하지 않음.
- `DamageDefinition.bShouldTriggerHitReaction` 은 데이터 에셋 쪽(적 공격 정의) — 바꾸면 모든 피격 반응이 함께 바뀜.
- 구현 `untouchable`(실험): 엔진 표준 `AActor.bCanBeDamaged`(+0x5A, 비트 0x04 — FBoolProperty 의 ByteMask 를 읽어 확인) 를 끔.
  게임의 GAS·DamageDealer 경로가 이 값을 보지 않으면 효과 없음 — 그것을 보는 실험.
- 기존 `god`(체력 상한 채우기)는 피해는 받되 죽지 않는 방식. 피격 경직·넘어짐은 남는다.

### 결과와 v2 (2026-10-03)
- ghost v1 (TeamID + Faction): 적이 공격하지 않음, **나도 못 때림**. → v2: TeamID 만 (감지는 팀, 피해는 Faction 으로 따로 볼 가능성).
- untouchable v1 (`bCanBeDamaged`): **효과 없음** — 게임의 피해는 이 값을 보지 않음. → v2: 적 공격은
  `DamageDealerBox/Capsule/SphereComponent`(도형 컴포넌트) — 겹침은 양쪽 모두 `bGenerateOverlapEvents` 일 때만 생기므로
  주인공 소유 PrimitiveComponent 전부(+0x25B 비트)를 끔. 부작용: 그동안 주인공의 겹침 트리거(문·줍기 범위·죽는 물) 반응 없음.

### 최종 (2026-10-03)
- ghost v2 (TeamID 만): **적이 여전히 공격** → 감지·피해 모두 `Faction` 판정, 대칭. 비대칭(적은 못 보고 나는 때림)은 데이터 쓰기로 불가.
  → v1 으로 되돌려 **"탐험용 고스트 — 켜는 동안 공격도 막힘"** 으로 남김(검증됨: 적이 무시).
- untouchable v2 (겹침 끄기): **효과 없음**. 적 타격은 겹침이 아님(트레이스/태그). 피격 무시는 태그(게임 안 할당) 또는 코드
  패치(B 단계)가 필요 → 토글 제거, .spec/CHEATS.md "What does not work" 에 기록.

