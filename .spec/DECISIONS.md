# 결정 기록

각 항목: 무엇을 정했나 · 왜 · 버린 대안. 번호는 바꾸지 않는다.

## D1. Phase 0 은 바깥 프로세스로 (dungeons2-mod 구조 재사용)

- 결정: DLL 주입 없이 `ReadProcessMemory`/`WriteProcessMemory`. 코드는 dungeons2-mod
  에서 복사해 고침.
- 이유: 같은 UE5 + GAS 게임이라 이름 풀·속성 해석·hold·패널을 그대로 쓸 수 있다.
  사용자가 조사 결과의 권고(외부 패널 → 미니맵 오버레이 → 필요하면 UE4SS)를 승인.
  안티치트가 없어 주입도 가능하지만, 외부 방식은 게임 파일을 하나도 건드리지 않고
  UE4SS 의 dwmapi.dll 프록시와 충돌하지 않는다.
- 버린 대안: UE4SS Lua 부터 시작 — 게임 안 UI 는 좋지만 치트 검증 도구(디버그 탭,
  원래 값 복구)를 새로 만들어야 함. ROADMAP 5 로 미룸.
- combolands-mod(MelonLoader/Unity), big-dragon-mod(CDP/웹)는 엔진이 달라 해당 없음.

## D2. 빌드별 앵커 표를 두지 않는다

- 결정: dungeons2-mod 의 `anchors.rs` (버전 → GEngine RVA·체인 오프셋 표)를 버리고,
  붙을 때마다 FNamePool·GEngine 을 찾고 체인을 리플렉션 이름으로 배운다.
- 이유: Steam 은 자동 업데이트가 잦고 (조사 시점에 막 패치됨, CE 테이블은 이미 낡음),
  빌드마다 사람이 doctor 결과를 붙여 넣는 절차가 필요 없게 하려고. 이름으로 찾으면
  틀린 객체에 쓸 위험도 줄어든다 — 이름이 안 맞으면 거부.
- 대가: 붙을 때 .data(약 6 MiB) 스캔 비용. 탐색이 유일하지 않으면 동작하지 않음.
- 알 수 없는 빌드를 거부하지 않는다 — 대신 모든 단계가 이름·클래스 검사를 통과해야 한다.

## D3. 솔로 게이트 대신 주인공 게이트

- 결정: 싱글 전용 게임이라 플레이어 수 대신 "조작 중인 폰이 `CharlieCharacterHero`"
  를 게이트로. 닫히면 토글은 **일시 정지** (dungeons2-mod 처럼 끄고 복구하지 않음).
- 이유: 게이트는 로딩·메뉴·시네마틱마다 닫힌다. 그때마다 꺼지면 사용자가 다시 켜야 함.
  쓰기 대상이 주인공임을 보장하는 것이 목적 — 적·NPC 의 ASC 에 쓰지 않는다.
- 복구(`restore`, 패널 닫기)도 게이트가 열려 있을 때만.

## D4. 속성은 이름으로, 세트는 `*` 허용

- 결정: dungeons2-mod D4 와 같이 이름으로 찾되, 세트 이름을 모를 때 `*` 로 "그 이름이
  유일한 세트". 둘 이상이면 거부하고 세트를 적으라고 말한다.
- 이유: 세트 클래스 이름은 실행 파일에서 찾았지만(ARCHITECTURE) 어떤 속성이 어느 세트에
  있는지는 게임을 켜야 안다. 유일성 검사로 안전하고, `doctor` 가 해석된 세트를 보여 준다.

## D5~D9: dungeons2-mod 에서 그대로

- 치트는 표 한 곳에서 (D5), 원래 값은 디스크에 먼저 (D6), 설정 유지는 사용자가 켠 것
  기준 (D8), 패널은 별도 창 + 폴링한 F8 (D9). 이유는 `~/dungeons2-mod/.spec/DECISIONS.md`.

## D10. 데이터는 설치 루트의 `Mods\`

- 결정: `...\steamapps\common\Hell Is Us\Mods\`. `HellIsUs\` 안이 아니라 옆.
- 이유: dungeons2-mod D7 과 같은 자리. Steam 무결성 검사는 배포 파일만 본다.
- 열린 문제: UE4SS·pak 모드도 쓰게 되면 이름이 겹칠 수 있다 — `Mods\hiumod\` 로 옮길지
  dungeons2-mod 와 함께 사용자에게 물을 것.

## D12. 효과 없는 계수 속성은 일반 필드로 대체

- 결정: GAS 속성 쓰기가 값은 유지되는데 효과가 없으면, 그 결과가 실제로 쓰이는 일반
  UPROPERTY(이동 컴포넌트 `MaxWalkSpeed`, 액터 `CustomTimeDilation`)를 쓴다.
  `Session::add_fields` — 소유 객체의 클래스를 먼저 확인(주인공 / CharacterMovementComponent),
  4바이트 float 만, 유한한 값만, 원래 값 기록은 속성과 같은 경로.
- 이유: 첫 플레이 테스트에서 계수형 속성 5개가 실패. GAS 는 수정자가 걸린 속성을
  FAggregator(자체 BaseValue)로 다시 계산하고, 피해 계산은 GameplayEffect 실행 때 캡처한
  값을 쓴다 — 속성 세트 메모리만 바꿔서는 닿지 않는 경로. Endurance 처럼 게임이 매번 직접
  읽는 값만 효과가 있었다.
- 버린 대안: 집계기 BaseValue 직접 쓰기 — `ActiveGameplayEffects.AttributeAggregatorMap`
  은 UPROPERTY 가 아니라 리플렉션으로 찾을 수 없고, 바깥 프로세스에서 TMap 을 고정 오프셋으로
  따라가야 함. Phase 1(UE4SS 에서 GameplayEffect 적용)에서 다시 본다.
- 보너스: `attack_speed`, `dodge_speed` 둘 대신 `hero_time` 하나 — 주인공만 빨라지고
  적은 그대로일 것으로 기대 (공격·회피·이동이 함께). 아직 플레이로 확인하지 않음.

## D13. 미니맵 창은 eframe 이 아니라 레이어드 Win32 창

- 결정: 미니맵은 별도 스레드의 `WS_EX_LAYERED | WS_EX_TRANSPARENT` 창, 직접 만든 작은
  래스터라이저(raster.rs)로 그려 `UpdateLayeredWindow`.
- 이유: eframe 은 루트 창(패널)이 숨겨지면 프레임을 돌리지 않는다(dungeons2-mod D9).
  미니맵은 패널을 F8 로 숨긴 동안에도 그려져야 한다. 레이어드 창은 포커스·클릭을 가져가지
  않아 게임이 마우스를 계속 잡는다. 240px 원 하나라 소프트웨어 그리기로 충분하다.
- 버린 대안: eframe 의 두 번째 viewport (위 이유), DX12 훅 오버레이 (주입 필요, D1).

## D14. 미니맵 아이콘은 SVG + resvg

- 결정: 종류별 아이콘을 `assets/icons/*.svg` 로 두고, 바이너리에 넣어(`include_str!`) 시작할 때 resvg 로
  한 번 래스터화. 그리기는 미리 곱한 비트맵 블릿.
- 이유: 사용자 제안. 점보다 종류가 한눈에 보이고, SVG 파일만 바꾸면 모양을 바꿀 수 있다. resvg 는 순수 Rust 라
  빌드 도구가 늘지 않는다 (default-features 끔 — 텍스트·래스터 이미지 기능 불필요).
- 대가: 의존성 추가(usvg, tiny-skia 등). 실패하면 예전처럼 점으로 그린다.

## D11. 미니맵은 지도 이미지 없이 먼저

- 결정: 1단계 미니맵은 위치·방향·지나온 경로·사용자 마커만 그린다.
- 이유: 지도 텍스처는 AES 암호화된 pak 안에 있고, 추출 에셋은 배포할 수 없다.
  경로+마커만으로도 "지도 없는 게임에서 길 잃지 않기"라는 목적의 대부분을 이룬다.
- 다음: 게임 자체의 월드맵 텍스처와 숨은 나침반 위젯을 쓰는 방법 (ROADMAP 3·5).
