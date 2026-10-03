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

폴더는 **역할(층)** 으로 나눈다. 아래 층은 위 층을 모른다 (unreal → read → guide/cheat → map → engine → ui).
모든 모듈은 `lib.rs` 에서 최상위로 다시 내보내므로 `crate::goals`, `hiumod::mem` 처럼 **모듈 이름만으로** 부른다
(어느 폴더에 있는지는 경로에 드러나지 않음 — 옮겨도 호출부가 안 바뀐다).

```
src/
  main.rs  cli.rs       CLI 명령 (doctor/list/get/set/hold/restore/pose/ui), 인자 해석
  engine/               mod.rs (Engine 루프: 붙기·게이트·hold, 파생값 1초마다·Arc 공유) · attached.rs (붙은 게임과
                        그 질의·캐시, extras 의 Reach 구현) · snapshot.rs (패널·오버레이가 읽는 유일한 것)
  unreal/               게임 메모리와 언리얼 리플렉션
    mem.rs              Memory trait, 포인터 사슬, 테스트용 Fake
    names.rs anchors.rs FNamePool·GEngine 찾기, 클래스 이름·계보·속성
    gobjects.rs         GUObjectArray
    player.rs           리플렉션으로 체인 학습, 주인공 게이트, 위치·방향
    usmap.rs probe.rs   .usmap 쓰기, doctor 하위 명령
  read/                 리플렉션으로 읽는 세계
    actors.rs           미니맵 액터 분류 (Kind 6 / Sub 23)
    attr.rs             속성 세트 읽기·쓰기 (Session)
    knowledge.rs        아는 사실·태그·조사
    terrain.rs obstacles.rs navmesh.rs geometry.rs   지형 높이, 장애물 껍질, 내비메시(A*+funnel), 정적 메시 윤곽
  cheat/                cheats.rs (표) · hold.rs (원래 값 기록·복구) · extras.rs (주인공 밖 대상; 게임은 Reach 트레이트로만)
  guide/                어디로, 왜
    goals.rs            안내 목표 (페이로드·NPC·퀘스트 아이템), Gate
    quests.rs           퀘스트 저널·비밀(선행·미스터리·타임루프), 시간 예산 읽기
    survey.rs           tools/survey 결과 (Mods\survey\*.json) 조회
    missables.rs        놓칠 수 있는 것 (assets/missables.tsv)
    pathfind.rs         장애물 격자 A* (내비메시 없을 때)
    target.rs           안내 대상 고르기 (자동·건너뛰기·막힘 → 여는 것), 순환 키
  map/                  지도 상태와 그리기
    minimap.rs          MapState (경로·설정·레이어), minimap.txt — pins·view 를 다시 내보냄
    pins.rs view.rs     지도 핀 (PinKind 24종·Marker) · 표시 방식·지형 모드·투영 (View)
    canvas.rs           미리 곱한 알파 픽셀 버퍼, 도형, 벡터 글꼴
    compass.rs          나침반 띠, 층 표시(흐림·위아래 화살표)
    raster.rs           draw_map (한 프레임) — canvas·compass 를 다시 내보냄
    relief.rs icons.rs  지형 굽기, assets/{icons,pins}/*.svg 래스터화
  i18n/                 게임 언어 따르기 (.spec/I18N.md): culture.rs (프로필의 TextCulture) · names.rs (게임 고유명사:
                        Mods\locale) · text.rs (모드 문구 표 assets/i18n) · fill.rs (trf! 실행 시 채움). tr!/trf! 매크로
  infra/                log.rs journal.rs (hiumod.log) · paths.rs (Mods\ 폴더, 모든 파일 경로의 뿌리) ·
                        settings.rs · verify.rs · backup.rs (세이브 백업) · memstat.rs (자기 메모리)
  game/                 밖에서 본 게임: locate.rs (설치), launch.rs, process.rs (RPM/WPM)
  ui/                   패널·오버레이 (Windows 전용)
    mod.rs              Request·Shared·worker 스레드·백업/메모리 로그 스레드
    panel/              eframe 패널: mod.rs (틀·헤더·탭·콘솔 창) + 탭마다 한 파일
                        groups · map · guide · quests · collect · saves · debug
    overlay/            오버레이 스레드: mod.rs (프레임 루프·창·키) · route.rs (목표까지 경로, 내비메시→격자, 스레드)
                        · bake.rs (지형 굽기) · hud.rs (핀 목표·나침반 표시·추적기 줄)
    tracker.rs pen.rs   퀘스트 추적기, GDI 한글 글자
    console.rs hotkey.rs layered.rs tw.rs   드롭다운 콘솔, 단축키·창 순서, 레이어드 창, 카드 레이아웃
assets/                 icons/ (종류 23) · pins/ (핀 24) · i18n/ (모드 문구 번역) · missables.tsv — 코드에 박지 않는 데이터
tools/survey/           C# + CUE4Parse 조사기 (결과는 커밋 안 함)
examples/               일회용 탐침 (gitignore)
```

규칙
- 새 모듈은 역할 폴더에 넣고 그 폴더 `mod.rs` 와 `lib.rs` 의 `pub use` 에 한 줄씩.
- 파일 경로는 `paths::data_dir()` 에서만 시작. 데이터·아이콘은 `assets/` 파일로.
- UI 는 게임 메모리를 읽지 않는다 (Snapshot 만). 안내 규칙은 guide/, 그리기는 map/ 에.
- 한 파일이 ~700 줄을 넘으면 관심사로 나눈다 (panel → panel/, raster → canvas/compass).

좌표: UE X 앞, Y 오른쪽, Z 위, yaw 는 +X 에서 +Y 쪽으로(위에서 보면 시계 방향).
**게임의 북쪽 = 월드 −Y (yaw 270)** — 게임 나침반과 비교해 확인 (`MapState.north_yaw`). 동쪽 = +X.
월드 이름 = 폰 → OuterPrivate(레벨) → OuterPrivate(월드) 의 이름.
로드된 레벨 = `World.Levels` (+0x178, 리플렉션). 레벨의 액터 = `ULevel::Actors` (+0xA0, 리플렉션 밖 —
주인공 레벨에서 주인공을 담은 유일한 TArray 로 찾음). 빌드 24045435: 레벨 140, 액터 10,671.

패널 스레드 규칙: **게임 메모리는 worker 스레드만 만진다.** 미니맵도 스냅숏만 읽는다. `Attached` 는 `RefCell`
을 가지므로 Sync 가 아니다 — worker 밖으로 넘기지 않는다.

## 테스트 고정물

`anchors::tests::image()` 가 PE 헤더·쓰기 가능 섹션·이름 풀·GEngine·GameInstance 를
가짜 메모리에 만들고, `player::tests::world()` 가 그 위에 로컬 플레이어·컨트롤러·
주인공·ASC·위치를 얹는다. 클래스·속성은 `names::fixture::Pool` 로 (`class`, `inherit`).
