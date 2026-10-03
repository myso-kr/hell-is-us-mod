# 작업 절차

## 빌드와 테스트

```
cd %USERPROFILE%\hell-is-us-mod
cargo fmt                                  # rustfmt.toml: 한 줄 120자
cargo clippy --all-targets -- -D warnings  # CI와 같은 기준
cargo test                                 # 42개, 게임 없이
cargo build --release                      # target\release\hiumod.exe
```

## 실행 중인 패널 교체

`target\release\hiumod.exe` 가 실행 중이면 release 빌드의 링크가 실패한다.
켜진 치트가 있으면 **사용자에게 닫아 달라고 한다** (전투 중에 갑자기 무적이 풀리면
안 됨). 그동안 확인은 debug 빌드(`target\debug\hiumod.exe doctor`, 읽기 전용)로.

## 게임 상태 확인 (모두 읽기만 함)

```
hiumod doctor          # 설치·빌드·앵커·레이아웃·체인·게이트·위치·속성·치트 표
hiumod list            # 주인공의 모든 속성 세트와 속성의 현재 값
hiumod get Endurance EnduranceCap
hiumod pose            # 위치·방향을 0.5초마다 (Ctrl+C 로 끝)
```

- 메인 메뉴에서는 앵커(이름 풀·GEngine·레이아웃)까지만 ok 이고 `player:` 에서 멈춘다.
  정상이다 — 세이브를 불러온 뒤 다시.
- 붙을 때마다 이미지의 쓰기 가능 섹션을 훑는다. 게임이 막 켜진 직후에는 GEngine 이
  아직 없을 수 있다 ("still starting up?").

## 새 치트 추가

1. `hiumod list` 로 속성의 정확한 이름을 찾는다.
2. `src/cheats.rs` 의 `mod a` 에 상수(`any("Name")`, 이름이 여러 세트에 있으면
   `attr("SetName", "Name")`)를, `CHEATS` 에 행을 추가한다. 처음엔 `verified: false`.
3. `cargo test` — 표 검사 테스트가 id 중복·범위·복구 누락을 잡는다.
4. `doctor` 의 `cheat table:` 이 ok 인지 → 패널에서 켜고 디버그 탭 → 사용자가 ✓.

## 게임 업데이트 대응

이 도구에는 빌드별 숫자가 없다. 업데이트 후 할 일은 확인뿐이다.

1. 게임을 켜고 세이브를 불러온 뒤 `hiumod doctor`.
2. 실패한 줄에 따라:
   - `name pool` / `GEngine` → 탐색 조건이 깨짐. `names.rs::discover`,
     `anchors.rs::find_engine` 의 조건을 확인 (후보가 0개인지 여러 개인지가 메시지에 나옴).
   - `FField layout` → 엔진이 올라갔다. `names::LAYOUTS` 에 새 후보를 추가.
   - `no property X` → 엔진/게임이 속성 이름을 바꿨다. `player.rs` 의 이름을 고친다.
   - `cheat table: not in the game` / `in N sets` → `cheats.rs` 행을 고친다.
3. `.spec/ANCHORS.md` 의 빌드 표에 새 빌드와 결과를 적는다.

## 커밋

- 메시지 끝에 시스템이 지정한 Co-Authored-By 줄.
- 게임 파일(.exe/.dll/.pak/.utoc/.ucas/.uasset/.sav)과 `Mods/` 는 절대 커밋하지 않는다
  (.gitignore + CI hygiene). 추출한 텍스처·AES 키도 마찬가지.
