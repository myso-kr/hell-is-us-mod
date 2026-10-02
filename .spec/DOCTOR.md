# doctor 확장 — 게임 데이터 조회 도구 (2026-10-03)

요청: "UE 를 이용해 메모리 스캔 doctor 를 인게임 데이터 기반으로 더 충실하게 조회." 효과를 거는 용도가 아니라 조회 도구.

## 결정: UE4SS 없이, 바깥에서
UE4SS 의 조회 기능(오브젝트 덤프, Live View, SDK 헤더)은 엔진 **반사 데이터**에서 나온다 — 이 크레이트가 이미 바깥에서
걷는 것과 같다(FNamePool, GUObjectArray, UStruct/FField). 반사 밖(네이티브 필드, 예: 아이템 수량 +0x48)은 UE4SS 덤프도
이름을 모른다. 그래서 D1(게임 파일 무수정)을 유지하고, 반사 밖은 **빈 구간 표시 · 변화 감시 · 값 스캔**으로 찾는다.

## 명령 (모두 읽기만, 결과는 `Mods\doctor\` 에도 저장)
| 명령 | 하는 일 |
|---|---|
| `doctor inspect <대상> [깊이] [gaps]` | 라이브 객체의 반사 필드 전부를 타입별 값으로(구조체 펼침, 배열 앞 8개, 포인터는 깊이만큼). `gaps`: 필드 사이 네이티브 바이트를 u32(+float 해석)로 |
| `doctor find <텍스트>` | 모든 클래스·구조체(≈14,000)에서 속성 이름 검색 — `Owner.Prop 타입 @+오프셋 (크기)` |
| `doctor dump [접두어…]` | SDK 형식 목록(기본 Charlie, Story → 471개, sdk.txt). 구조체·배열 원소 타입 포함 |
| `doctor watch <대상> [초]` | 0.1 초마다 객체 메모리를 비교, 바뀐 필드를 이름으로·네이티브는 오프셋으로, 시각과 함께 |
| `doctor scan <대상> <값>` / `scan next <값>` | 객체 안에서 값의 자리(u32/f32/f64) → 게임에서 값을 바꾼 뒤 남는 자리만 |

대상: `hero`, `controller`, `asc`, `sets[:N]`, `inventory`, `items[:N]`, `save`(최신 CharlieSaveGame), `world`(WorldSettings),
`enemy[:N]`, `0x주소`, 클래스 이름`[:N]`.

## 실측 (build 24045435)
- `inspect items:3 0 gaps` → `CharlieInventoryUseableItem` 의 `· native +0x48..: 0x9 …` — 수량 9 가 반사 밖 첫 워드로 보임.
- `scan items:3 9` → 1곳(+0x48), `scan next 9` → 그대로 1곳.
- `find Quantity` → 18개 (`InventoryItem.Quantity`, `ItemData.QuantityMax @+0xd8`, `CraftIngredient.Quantity` …).
- `dump` → 471개 클래스·구조체, 2,078줄.

## 사례: 무기 경험치 (2026-10-03)
1. `find WeaponCurrentXP` → 세이브 `CharlieInventoryItemState.WeaponCurrentXP`(한손검 90, 쌍도끼 1560).
2. `scan CharlieInventoryWeaponItem:0 90` → 4곳, 쌍도끼 1560 → 2곳(+0x13C, +0x144).
3. `inspect … gaps` 로 두 무기 꼬리 비교 → 쌍도끼는 다음 레벨 기준 +0x148 = 0 (등급 상한 — 처치해도 안 오름).
4. 한손검으로 처치하며 `watch`: +0x13C 90→265(누적), +0x140 90→5(레벨 안), +0x148 260→520(다음 기준),
   +0x130 0→1(레벨), +0x138 3(상한), +0x14C/0x150 700→725(능력치). → `weapon_xp` 치트.

## 새 치트를 찾는 순서
1. `find` 로 이름 후보 → 2. `inspect <대상> 1 gaps` 로 값과 빈 구간 → 3. 게임에서 그 값을 움직이며 `watch` 또는
`scan` / `scan next` → 4. 오프셋을 반사 필드 기준(예: `ItemData`+8)으로 표현해 코드에.
