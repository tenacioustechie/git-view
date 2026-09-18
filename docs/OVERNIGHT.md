# Overnight implementation runbook

Prepared for the night of 18–19 September 2026 in Australia/Melbourne. This is an execution plan, not an active or scheduled coding run. Use elapsed checkpoints after an actual kickoff; no finish time has been promised.

## Target and order

The first integrated deliverable is **JSON project file → repository status dashboard → Pull all → dirty repositories skipped with individual results**. This is the user's confirmed priority. Build the single-repository history and commit workflow alongside it. Complete M1 before spending time on merge/squash-merge or visual polish.

Use one coordinator and at most three concurrent workers, matching this session's four available agent slots. Workers inherit the current model settings unless the user requests otherwise. Assign bounded work and exclusive file ownership. Codex supports explicit subagent requests and recommends care with concurrent writes because they can conflict; this runbook uses a single owner for shared files. See [official subagent guidance](https://learn.chatgpt.com/docs/agent-configuration/subagents).

An overnight run requires an active execution session, sufficient account/runtime availability, a machine that remains awake, and resolved prerequisite access. Writing these files does not create such a session. Do not promise work will run after the session stops.

## Preflight while the user is available

| Check | Current evidence | Next action when implementation starts |
| --- | --- | --- |
| Stack | Tauri/Rust/TypeScript accepted | Use React/Vite unless corrected |
| Rust | Not found in PATH or default install locations | Install via supported toolchain setup with any required runtime permission |
| Node / pnpm | Node 22.22.0 / pnpm 9.12.0 available | Check compatibility with selected packages; pin working versions |
| Apple build tools | Command Line Tools path available | Compile the minimal app before parallel feature work |
| Downloads | Session network is restricted | Resolve real dependency-download permission prompts during scaffold |
| GUI launch | May require runtime permission | Run a minimal native smoke test before leaving it unattended |
| Repository | Empty, no commits | Preserve planning docs; coordinator owns any initial commit/checkpoint |
| Windows/Linux CI | No configured remote/runner established | Add workflow files; report unexecuted platform checks honestly |

Do not change the user's global agent settings, disable sandbox protections, or assume permission for protected installations. If a prerequisite is blocked, continue independent UI/contracts/docs work and record the precise blocker. Mocked UI progress is not a completed native Git application.

## Ownership

| Owner | Writes | Responsibility |
| --- | --- | --- |
| Coordinator | Manifests/lockfiles, `src/app`, `src/api`, `src/components`, Rust contracts/registration/launch/catalog/operations, CI, handoff docs | Scaffold, shared contracts, JSON project loader, CLI, operation orchestration, integration and final verification |
| Git worker | `src-tauri/src/git/**`, backend fixtures/tests | Process runner, canonical identities/locks, status, history, diff, preflight and Git actions |
| Repository worker | `src/features/repository/**`, `src/graph/**`, adjacent tests | Repository screen, DAG, details, changes, staging/commit/action controls |
| Group worker | `src/features/groups/**`, adjacent tests | Project dashboard, filters, selection, batch preview/results, project editing UI |

Workers may read all files. They request coordinator edits to shared contracts/components/manifests. They do not reset files, change branches, stage unrelated changes, or commit other workers' work. Shared workspace edits are visible immediately; no cherry-picking is needed in this mode. A worker reports exact files changed and test evidence before the coordinator integrates the slice.

## Dependency and checkpoint schedule

The following is an 8-hour planning allocation, not a delivery guarantee. Do not end the run merely because a checkpoint estimate passes; record progress and continue within the actual session's limits. If a time limit is set, use the last available interval for a coherent handoff.

| Elapsed target | Coordinator | Git worker | Repository worker | Group worker | Gate |
| --- | --- | --- | --- | --- | --- |
| 0–45 min | Toolchain, runnable scaffold, contracts, shared controls | Start after skeleton/contracts exist | Start after skeleton/contracts exist | Start after skeleton/contracts exist | Native shell compiles; interfaces and fixtures agree |
| 45 min–2 h | Project loader + CLI + repository registry | Runner, repository identity, status, locks | Graph/layout fixtures and screen | Real project dashboard wiring | Real JSON group opens with correct statuses |
| 2–4 h | Batch service, preflight/events, integrate dashboard | Fetch/pull preflight/actions; tests | Commit details/diff UI; staging form | Pull all, skip/results/retry UI | Mixed group pulls correctly; dirty repo untouched |
| 4–6 h | Native integration; launcher; CI setup | History/diffs, stage/unstage/commit/push/branch actions | Wire real APIs; selection/paging/errors | Project editing; external file changes; group checks | Real repo history → stage → commit → pull/push |
| 6–7 h | Resolve integration defects; evaluate M1 checklist | Fix concrete failures; M2 only if gates pass | Fix UI behavior and graph defects | Cross-review workflows; accessibility defects | M1 accepted or accurately marked incomplete |
| 7–8 h | Local package, smoke tests, handoff | Review mutation correctness | Review repository workflows | Review group workflows | Runnable artifact and evidence, or specific remaining blockers |

Tests accompany each domain implementation. The coordinator should reuse an available worker for a read-only review of another worker's code, keeping at most three workers active. No additional agent layer is needed for this project.

## Task backlog and acceptance

| ID | Owner | Depends on | Deliverable and acceptance |
| --- | --- | --- | --- |
| F1 | Coordinator | Preflight | Native Tauri app opens, agreed scripts run, dependency versions locked |
| F2 | Coordinator | F1 | Wire types and fixtures compile on both sides; shared invoke/error wrapper exists |
| B1 | Git | F2 | Real status and identity handle clean/dirty/unborn/detached/worktrees/paths with spaces |
| P1 | Coordinator | F2, B1 | JSON loader resolves relative paths, retains missing members, rejects invalid versions |
| G1 | Group | F2; live integration P1 | Dashboard shows actual group state, filters by branch, opens a repository |
| B2 | Git | B1 | Fetch/pull obey explicit policies and reject dirty/stale targets in fixtures |
| P2 | Coordinator | B2, P1 | Batch plans snapshot candidates, schedule jobs, and retain individual results |
| G2 | Group | G1, P2 | Pull all passes success/dirty-skip/failure scenario and supports retry |
| R1 | Repository | F2 | DAG from fixtures preserves parent edges and page continuation |
| B3 | Git | B1 | Snapshot history, details, changed paths, and bounded diffs tested |
| R2 | Repository | R1, B3 | Actual commit selection displays correct metadata/parent/files/diff |
| B4 | Git | B1 | Stage/unstage/commit/push/branch actions pass meaningful state assertions |
| R3 | Repository | R2, B4 | Working changes can stage one file, commit only staged content, retain draft on failure |
| P3 | Coordinator | F1, P1 | Cold/warm CLI launch resolves invoking cwd, focuses correct mode, reports failures |
| G3 | Group + Coordinator | G1, P1 | Create/edit/save project; changed-file conflict handled without overwrite |
| Q1 | Coordinator + reviewers | G2, R3, P3 | Acceptance suite and native smoke; local build artifact and platform evidence |
| M2 | Git + Repository | Q1 green | Ordinary merge and squash-merge with honest conflict lifecycle; see Git spec |
| M3 | Git + Repository | M2, rewrite recovery design | Required Combine commits workflow: preview a linear tip sequence, edit message, create backup, rewrite, and verify recovery; separate from the overnight M1 target |

If the run falls behind, cut discovery, animation, advanced search, hunk staging, M2/M3, and optional project editing polish first. Keep the core group integration, DAG correctness, real mutation tests, and truthful handoff. Any missing M1 item remains incomplete in the report; a cut is not a passed criterion.

## Worker prompts

### Git worker

Read `docs/GIT_ENGINE.md`, `docs/CONTRACTS.md`, and `docs/ACCEPTANCE.md`. Own only `src-tauri/src/git/**` and agreed backend fixtures/tests. Deliver B1 then B2 before B3/B4 so the group workflow integrates early. Use real temporary repositories and local remotes. Never mutate personal repositories for tests. Coordinate interface changes before editing shared types. Report completed functions, test commands/results, limitations, and files changed after each slice. Continue with assigned work after each report.

### Repository UI worker

Read `docs/PRODUCT.md`, `docs/ARCHITECTURE.md`, and `docs/CONTRACTS.md`. Own `src/features/repository/**` and `src/graph/**`. Build against coordinator fixtures first, then wire the same contract to real APIs. Focus on graph parent correctness, paging, commit inspection, whole-file stage/unstage, and retained commit drafts. Do not create a separate API layer or invent shared types. Report completed workflows, test evidence, and blockers per slice.

### Group UI worker

Read `docs/PROJECT_FILES.md`, `docs/PRODUCT.md`, and `docs/CONTRACTS.md`. Own `src/features/groups/**`. Prioritize real project loading/status and Pull all with dirty skips and per-repository results. Make the exact batch scope visible and preserve selection/navigation. Backend plans decide eligibility. Coordinate project saving and operation services with the coordinator. Report fixture coverage, live API integration, test results, and limitations per slice.

## Kickoff prompt for the implementation session

```text
Build Git View in this workspace using the plans in README.md and docs/.
Read DECISIONS, PRODUCT, PROJECT_FILES, ARCHITECTURE, GIT_ENGINE,
CONTRACTS, OVERNIGHT, and ACCEPTANCE before assigning work.

Use one coordinator and up to three subagents with the file ownership
in OVERNIGHT.md. Scaffold and freeze shared contracts before concurrent
edits. Keep the agreed Tauri/Rust/TypeScript stack; React is the default UI.

The first integration gate is a real JSON project dashboard and Pull all
that skips dirty repositories and reports each outcome. Complete M1,
including the single-repository DAG and commit workflow, before M2 stretch
features. Test Git mutations only in disposable fixtures with local remotes.

Continue implementing and integrating without stopping at a plan. Follow
actual runtime permissions for installs/downloads/GUI access; resolve setup
early and record blockers. Do not publish releases or push branches.
Update docs/BUILD_STATUS.md after each integration gate. Before handing off,
record exact commands/results, the runnable artifact or dev command, tested
platforms, unfinished work, and the next concrete action. Do not call mocked
UI or unrun CI a verified cross-platform application.
```

## Checkpoint record to create during the build

`docs/BUILD_STATUS.md` should contain the current commit/diff state, completed task IDs, active worker ownership, agreed contract changes, exact test commands/results, platform/artifact status, blockers, and the next task. Update after each integration gate so a resumed session can continue without rebuilding the plan. Do not include credentials or private repository content.
