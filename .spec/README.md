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
| 부록 | [GUIDE.md](GUIDE.md) | 나침반 HUD·퀘스트 목표 안내: 웹·메모리 조사(퀘스트=사실 그래프, 저장 상태, 페이로드), 설계, 구현 기록 |
| 부록 | [CHEATS-RESEARCH.md](CHEATS-RESEARCH.md) | 치트 확장 조사: 기존 트레이너·테이블, 메모리 프로브(인벤토리 수량 +0x48), 계수 치트가 안 된 이유(D12), 데이터 / 코드 패치 / UE4SS 단계 제안 |
| 부록 | [DOCTOR.md](DOCTOR.md) | doctor 확장: inspect·find·dump·watch·scan — 반사 기반 조회와 네이티브 필드 찾기 (UE4SS 대신 바깥에서) |
| 부록 | [QUESTS.md](QUESTS.md) | 메인 스토리 길안내 조사 + §4 구현: FText 읽기, 선행(서브퀘스트), 퀘스트 저널·추적기·따라가기 |
| 부록 | [ITEMS.md](ITEMS.md) | 퀘스트 아이템 전수조사 설계: pak 의 WP 셀 → 오프라인 DB, 세이브 Guid 로 상태 판정, 런타임 합치기 |
| 부록 | [MAP.md](MAP.md) | 미니맵 3단계: 게임 에셋 조사(AES·retoc·월드맵 텍스처), 결론, 월드에서 지도 만들기 진행 기록 |

영어 참고 문서도 여기 있습니다 — [PLAN.md](PLAN.md) (무엇·단계), [ANCHORS.md](ANCHORS.md) (들어가는 길·게임 업데이트가
깨뜨릴 수 있는 것), [CHEATS.md](CHEATS.md) (치트·검증 기준). 어긋나면 코드와 ANCHORS.md 가 기준입니다.
`docs/` 는 GitHub Pages 홈페이지 전용입니다 (2026-10-03 통합).

형제 프로젝트: `~/dungeons2-mod` (Minecraft Dungeons II, 같은 구조의 원본),
`~/combolands-mod` (MelonLoader), `~/big-dragon-mod` (CDP).

마지막 갱신: 2026-10-02 · Steam 빌드 24045435 · hiumod 0.1.0 (doctor ok, 치트 미검증)
