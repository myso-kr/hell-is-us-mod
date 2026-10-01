# 다음 작업

우선순위 순. 각 항목의 **완료 기준**을 만족해야 끝난 것으로 본다. 게임이 업데이트되면
무엇보다 먼저 [RUNBOOK.md](RUNBOOK.md) 의 "게임 업데이트 대응" 을 한다.

## 1. 실제 게임에서 첫 연결 — **완료 (2026-10-02)**

- 무엇: 사용자가 게임을 켜고 세이브를 불러와 주인공을 조작할 수 있는 상태에서
  `hiumod doctor` → `hiumod list` → `hiumod pose`.
- 볼 것 (doctor 출력 순서):
  1. `name pool`, `GEngine` 이 ok — 실패하면 메시지대로 (`0 found` / `N candidates`).
  2. `FField layout` — 어느 후보가 뽑혔는지 기록.
  3. `hero:` 계보에 `CharlieCharacterHero` 가 있는지. 없으면 `player::HERO` 를
     실제 계보의 적절한 클래스로 바꾼다.
  4. `attributes:` 세트 목록과 `cheat table:` 결과. 해석 안 되는 이름은 `list` 로 찾아 고친다.
  5. `pose` 를 띄운 채 걸어 다니며 x/y 가 움직이고 yaw 가 카메라 방향을 따르는지.
- 실패 시: 메인 메뉴에서 `doctor` 를 돌리면 앵커 단계까지는 확인 가능 (폰 없음).
  레이아웃이 4개 다 안 맞으면 `examples/probe.rs` 로 GameEngine 클래스의 FField 를
  덤프해 직접 찾는다 (dungeons2-mod 에서 한 방법, ARCHITECTURE 참고).
- 완료 기준: doctor 전 항목 ok, STATUS·ANCHORS 의 "확인 못 한 것" 갱신.

## 2. 치트 검증 — **대부분 완료 (2026-10-02): 4개 확인, 11개 삭제, 3개 미시도.** 남은 3개는 아무 때나

(아래는 처음 계획. 결과는 STATUS 와 docs/CHEATS.md)

- 패널(`hiumod`)에서 하나씩 켜고 디버그 탭으로 원래 값·넣은 값·현재 값을 본다.
- 의심 지점:
  - `god`: 체력 = `EnduranceCap`, 최대 = `EnduranceMax` 라는 CE 테이블 해석이 맞는지.
    받는 피해 계수(`DamageTakenBoostCoefficient`)를 0으로 두는 쪽이 나을 수도.
  - `defense`: 기본값 0.3 을 피해 감소율로 보고 0–0.95 로 둠. 1 이상이면 맞을수록 회복될 수 있어 막았다.
  - 보너스형(기본 0)과 배수형(기본 1) 구분이 맞는지 — `speed` 0.5 가 1.5배인지 본다.
  - 다른 무적 방법: `PlayerAttributeSet.DamageTakenBoostCoefficient` (기본 0) 를 -1 로.
  - 쿨다운 0.05 (`NEAR_ZERO`) 가 이 게임에서도 필요한지.
  - CE 테이블 사용자 보고: 공유 코드를 훅한 "무한 체력"이 적도 무적으로 만듦. 우리는
    주인공 ASC 에만 쓰므로 해당 없어야 함 — 적이 안 죽는지 실제로 본다.
- 완료 기준: 각 행 `verified: true` 또는 원인과 함께 수정·삭제. docs/CHEATS.md 갱신.

## 3. 미니맵 오버레이 (큼) — **다음 작업**

- 1단계 (외부 창, 지도 이미지 없음): 패널과 같은 프로세스에 두 번째 창.
  - 항상 위, 클릭 통과(`WS_EX_TRANSPARENT | WS_EX_LAYERED`), 게임 창 모서리에 고정.
  - `Snapshot.pose` 로 그림: 중심 = 주인공, 회전 = 카메라 yaw, 지나온 경로(빵부스러기),
    사용자 마커(단축키로 현재 위치 찍기, 지역별 파일에 저장), 나침반 방위.
  - 지역 구분: 월드(레벨) 이름으로 경로·마커 파일을 나눈다 — `UWorld` 이름을 names 로.
  - 단축키: F7 미니맵 표시/숨김, 마커 찍기·지우기, 확대/축소.
- 2단계 (적·상호작용 표시): GUObjectArray 를 찾아 액터를 순회하고 클래스 이름으로
  골라 위치를 그린다 (`CharlieLymbicEntity`, 상자, 텔레포트 마커 등). 비용 큼 — 1~2 Hz.
- 3단계 (지도 이미지): 게임에 `Close/Middle/FarWorldMapSoftTexture`, `CompassSumg`,
  `ShowCompass` 가 있다. pak 은 AES 암호화 → AESDumpster 로 키 추출 → FModel 로
  텍스처와 월드↔지도 좌표 변환 데이터를 찾는다. **추출한 에셋은 저장소에 넣지 않는다.**
- 완료 기준(1단계): 걸으면 점이 움직이고, 회전이 카메라와 맞고, 마커가 재시작 후에도 남음.

## 4. 재화·소모품 (중간)

- 인벤토리는 `CharlieInventory*` 클래스 (CE 테이블: 아이템 수량 `[item+0x48]`).
  리플렉션으로 `CharlieInventoryItemStack` 의 수량 속성 이름을 찾아 `Kind::Set` 행으로.
- 완료 기준: 소모품 하나의 수량 변경이 저장·재접속 후 유지.

## 5. 게임 안으로 (큼, 별도 설계 필요)

- 후보: RE-UE4SS (Nexus #43, UE 5.5 설정 포함) — Lua 로 `CharlieCheatManager` 를
  살려 개발자 치트를 쓰거나, 게임 UMG 로 미니맵을 그린다 (Palworld PalMiniMap 방식).
- 업적 차단도 여기서: `CharlieAchievementsUnlockerSubsystem` 을 막는다.
- 바깥에서 안 된 치트(피해·방어·회복·무적 시간·쿨다운·경험치)는 여기서 GameplayEffect
  (또는 `CharlieCheatManager`)로 — docs/CHEATS.md 의 실패 목록이 후보.
- 결정 전에 DECISIONS D1 을 다시 읽을 것.

## 6. 릴리스 (작음)

- dungeons2-mod ROADMAP 5 와 같음: 태그 → Actions 에서 release 빌드 → zip + `.sha256` → draft.
