# 구조

## 게임

- Hell Is Us (Rogue Factor / Nacon), Steam AppID **1620730**, 실행 파일
  `HellIsUs\Binaries\Win64\HellIsUs-Win64-Shipping.exe` (155 MiB, 암호화 없음 —
  디스크에서 읽힘). Denuvo·EAC 없음.
- Unreal Engine **5.5.4** (PCGamingWiki; 실행 파일의 MegaLights 와 일치).
  버전 리소스는 `UE5-CL-0` 뿐이라 정확한 빌드 문자열은 실행 파일에 없음.
- 내부 코드명 **Charlie**: `CharliePlayerController`, `CharlieCharacterHero`,
  `CharlieHeroAbilitySystemComponent`, `CharlieGameInstance`, `CharlieCheatManager`, …
- 능력치는 GAS 속성. 네이티브 세트 클래스(실행 파일의 UTF-16 등록 이름):
  `EnduranceAttributeSet`, `LymbicAttributeSet`, `DamageAttributeSet`,
  `WeaponAttributeSet`, `LocomotionAttributeSet`, `PoiseAttributeSet`,
  `PlayerDefenseAttributeSet`, `PlayerAttributeSet`, `StanceAttributeSet`,
  `HealthAttributeSet`, `CharlieAttributeSet`, …
- **체력은 스태미나의 상한**: 맞으면 `EnduranceCap` 이 줄고, `Endurance` 는 그 아래에서
  움직인다 (CE 테이블의 "health" = EnduranceCap).
- 플레이어 블루프린트: `StoryHero_BP_C`. CE 테이블 기준 +0x688 ASC, ASC +0x1088
  SpawnedAttributes — **참고만 하고 코드에는 쓰지 않는다** (리플렉션으로 찾음).
- pak: IoStore, utoc 플래그 Compressed|Encrypted|Signed|Indexed. AES 키 공개된 것 없음.

## 메모리를 따라가는 길

```
이미지 .data 스캔
  ├─ FNamePool: 블록 0 이 "None","ByteProperty" 로 시작하는 유일한 풀   (names::discover)
  └─ GEngine:   클래스 계보에 "GameEngine" 이 있고 Default__ 가 아닌 객체를
                가리키는 전역 — 객체가 하나여야 함                      (anchors::find_engine)
FField 레이아웃: LAYOUTS 4개 중 GameEngine.GameInstance → GameInstance 가 되는 것

GEngine ─GameInstance→ ─LocalPlayers[0]→ ─PlayerController→ ─Pawn→ 주인공
   주인공 ─(AbilitySystemComponent 를 가리키는 속성)→ ASC ─SpawnedAttributes→ TArray<세트>
   주인공 ─RootComponent→ ─RelativeLocation (f64×3, cm)
   컨트롤러 ─ControlRotation (f64×3: pitch, yaw, roll)
```

이름 → 오프셋 해석은 `player::learn` 이 붙은 뒤 처음 주인공이 있을 때 한 번 하고
(`Attached.chain`), 이후 매 틱은 그 오프셋으로 포인터만 따라간다.

엔진 레이아웃 상수 (`names.rs`): UObject `ClassPrivate` +0x10, `NamePrivate` +0x18,
`OuterPrivate` +0x20; UStruct `SuperStruct` +0x40, `ChildProperties` +0x50. FField 쪽
(Next, Name, ElementSize, Offset_Internal)은 엔진 버전마다 달라 후보에서 고른다.
UE 5.6(dungeons2-mod 실측)은 0x18/0x20/0x34/0x48, **이 게임(UE 5.5)은 0x18/0x20/0x34/0x44** (2026-10-02 실측).

FName index = (블록 << 16) | (블록 안 오프셋 / 2). 엔트리 = u16 헤더(bit0 wide,
상위 10비트 길이) + 문자. FNamePool 블록 배열은 풀 +0x10.

## 쓰기 전 안전장치 (순서대로)

1. 주인공 게이트: 조작 중인 폰의 계보에 `CharlieCharacterHero` (`player::Chain::hero`).
2. 속성은 세트 클래스 이름 + 속성 이름(`*` 면 유일한 이름)으로 찾고, ElementSize 16.
3. 대상 첫 8바이트가 공유 속성 vtable — 모든 속성의 4/5 이상이 동의하고 게임 이미지 안.
   일반 필드는 vtable 대신: 소유 객체 클래스 확인 + 리플렉션상 크기 4.
4. 유한한 float 만.
5. 덮어쓰기 전에 원래 값을 파일에 기록 — 기록 실패 시 쓰지 않음 (`hold.rs`).

## 코드 지도

```
src/
  main.rs         CLI 명령 (doctor/list/get/set/hold/restore/pose/ui)
  cli.rs          인자 해석
  engine.rs       붙기(앵커 탐색)·체인 학습·게이트·hold·읽기 루프. CLI hold 와 패널이 공유
  anchors.rs      FNamePool·GEngine 탐색, FField 레이아웃 선택
  player.rs       리플렉션으로 체인 학습, 주인공 게이트, 위치·방향
  names.rs        FNamePool 읽기, 클래스 이름·계보·속성(상속 포함) 찾기
  attr.rs         속성 세트/속성을 이름으로 해석(`*` 포함), 검사된 읽기·쓰기 (Session).
                  일반 float 필드도 같은 Session 에 (`add_fields`, 레이블 Hero / Movement)
  cheats.rs       치트 표 (7행), 일반 필드 목록 HERO_FIELDS / MOVEMENT_FIELDS
  actors.rs       미니맵에 찍을 액터: 레벨 순회, 클래스 계보로 분류(classify), 1 Hz 스캔 + 매 스텝 위치,
                  다 쓴 것 거르기(Done: 적 체력 0, InteractionActionComponent.bHasBeenActivated),
                  세부 종류 Sub 22종 (적: 계열명, 아이템: 클래스 이름 접두사·단어, 문·퍼즐: 계보) — Kind 는 Sub 에서
  icons.rs        assets/icons/*.svg 를 include_str! 로 넣고 resvg 로 래스터화 (미리 곱한 ARGB)
  minimap.rs      미니맵 상태(경로·마커·설정·레이어, 월드별), 투영(View), minimap.txt 형식
  raster.rs       미리 곱한 알파 픽셀 버퍼에 원·고리·선·삼각형·N, draw_map (한 프레임)
  hold.rs         원래 값 기록(originals.txt), 부분 복구
  settings.rs     패널 설정(settings.txt)
  verify.rs       사용자 검증 기록(verify.txt)
  journal.rs      로그(hiumod.log)
  paths.rs        데이터 폴더: <설치 루트>\Mods\, 안 되면 %LOCALAPPDATA%\hiumod\
  mem.rs          Memory trait, 포인터 사슬, 테스트용 Fake 메모리
  log.rs          패닉하지 않는 log!/warn!
  game/
    locate.rs     Steam 라이브러리에서 설치 찾기, buildid 읽기
    launch.rs     steam://rungameid/1620730
    process.rs    프로세스 찾기·열기, ReadProcessMemory/WriteProcessMemory, Ctrl+C
  ui/             dungeons2-mod 와 같은 F8 패널 (worker·hotkey·eframe) + minimap 스레드:
                  ui/minimap.rs — 레이어드 창, UpdateLayeredWindow, 표시·마커 키 폴링 (패널에서 고름), 10초마다 저장
```

좌표: UE X 앞("북"으로 씀), Y 오른쪽, Z 위, yaw 는 +X 에서 +Y 쪽으로(위에서 보면 시계 방향).
월드 이름 = 폰 → OuterPrivate(레벨) → OuterPrivate(월드) 의 이름.
로드된 레벨 = `World.Levels` (+0x178, 리플렉션). 레벨의 액터 = `ULevel::Actors` (+0xA0, 리플렉션 밖 —
주인공 레벨에서 주인공을 담은 유일한 TArray 로 찾음). 빌드 24045435: 레벨 140, 액터 10,671.

패널 스레드 규칙: **게임 메모리는 worker 스레드만 만진다.** 미니맵도 스냅숏만 읽는다. `Attached` 는 `RefCell`
을 가지므로 Sync 가 아니다 — worker 밖으로 넘기지 않는다.

## 테스트 고정물

`anchors::tests::image()` 가 PE 헤더·쓰기 가능 섹션·이름 풀·GEngine·GameInstance 를
가짜 메모리에 만들고, `player::tests::world()` 가 그 위에 로컬 플레이어·컨트롤러·
주인공·ASC·위치를 얹는다. 클래스·속성은 `names::fixture::Pool` 로 (`class`, `inherit`).
