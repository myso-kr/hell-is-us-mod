# .spec — 이어서 작업하기 위한 문서

이 폴더는 작업을 멈춘 지점에서 다시 시작하기 위한 문서입니다. 사람이든 AI든,
처음 이 저장소를 여는 쪽이 **이 순서대로** 읽으면 바로 다음 일을 할 수 있게
쓰여 있습니다.

| 순서 | 문서 | 답하는 질문 |
|---|---|---|
| 1 | [STATUS.md](STATUS.md) | 지금 어디까지 됐고, 무엇이 확인됐고, 무엇이 안 됐나 |
| 2 | [ROADMAP.md](ROADMAP.md) | 다음에 무엇을, 어떤 순서로, 무엇을 기준으로 끝났다고 하나 |
| 3 | [RUNBOOK.md](RUNBOOK.md) | 빌드·실행·검증·게임 업데이트 대응을 실제로 어떻게 하나 |
| 4 | [ARCHITECTURE.md](ARCHITECTURE.md) | 코드가 어떻게 생겼고, 게임 메모리를 어떻게 따라가나 |
| 5 | [DECISIONS.md](DECISIONS.md) | 왜 이렇게 했고, 무엇을 해 봤다가 버렸나 |
| 부록 | [RESEARCH.md](RESEARCH.md) | 시작 전 조사: 게임·엔진·기존 모드·도구·미니맵 사례 (출처 포함) |

`docs/` 와의 차이: `docs/` 는 저장소를 보는 사람에게 설명하는 문서(영어)이고,
`.spec/` 은 작업을 이어 갈 사람을 위한 작업 문서(한국어)입니다. 둘이 어긋나면
코드와 `docs/ANCHORS.md` 가 기준입니다.

형제 프로젝트: `~/dungeons2-mod` (Minecraft Dungeons II, 같은 구조의 원본),
`~/combolands-mod` (MelonLoader), `~/big-dragon-mod` (CDP).

마지막 갱신: 2026-10-02 · Steam 빌드 24045435 · hiumod 0.1.0 (doctor ok, 치트 미검증)
