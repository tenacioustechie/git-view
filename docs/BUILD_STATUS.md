# Build status

## 2026-09-23: guided linear rebase complete on tested platforms

Source checkpoint `d9af4e65d99647280409f0f78c76aeeaac31df18`, based on A4
checkpoint `9fe0fb8`, adds guided linear
rebase onto an exact existing local or fetched remote branch, ordered preview,
backup and per-worktree record, stopped-state context, external-editor conflict
resolution, Continue, reviewed Skip, and Abort. The macOS arm64 and Windows 11
x64 verification, native desktop, release build, and package launch/install
gates pass on this exact checkpoint.

| Gate | macOS arm64 | Windows 11 x64 |
| --- | --- | --- |
| Frontend | 114 passed; typecheck/lint passed | 114 passed; typecheck/lint passed |
| Rust | 171 passed; one opt-in performance case ignored | 162 passed; one opt-in performance case ignored |
| Formatting and strict Clippy | Passed | Passed |
| Native desktop | 50/50 checks passed | 50/50 checks passed |
| Release package | `.app` built; packaged CLI/startup passed | NSIS built; installed CLI/native window passed |

Eight macOS / seven Windows focused real-Git rebase tests cover clean replay,
stopped conflict and reload, Continue, Skip, Abort, empty commit, metadata drift,
stale target/dirty state, and merge-commit rejection; macOS additionally checks
pre-rebase hook rejection. Both native reports exercise clean replay and conflict
Continue/Skip/Abort through the visible UI and real IPC, then assert final refs,
backup, and file bytes. Reports are `artifacts/native-macos.json` and
`artifacts/native-windows.json` (Windows original
`C:\codeme\git-view\.tools\GitViewNativeSmoke-befb5b289e3f4a09895f00eacb75c4e4.json`).
The Windows installed test package is retained at
`C:\codeme\git-view\.tools\installed-smoke-fc494b3671974c9a9906de8fc0acd36e`.

Commands included `pnpm typecheck`, `pnpm lint`, `pnpm test`, locked full Cargo
tests, `cargo fmt --check`, strict all-target Clippy, the native-smoke frontend
and custom-protocol builds, and `python3 scripts/native-smoke.py` on macOS.
Windows used `windows-sync.py`, `windows-verify.ps1`, `windows-release.ps1`, and
`windows-installer-smoke.ps1`. The unsigned packages were collected locally and
were not published:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `artifacts/Git-View_0.1.0_macos-arm64.zip` | 3,792,351 | `3b0278f3fb6cd784db8fcbe73b91e565d2364e0704557d37f7cd7bcfa5c50467` |
| `artifacts/Git-View_0.1.0_windows-x64-setup.exe` | 2,993,364 | `51d708b3f38c76436f8fe40566482ed5710e18263390c92ea44e5489ad96fc5e` |

`artifacts/manifest.json` records matching source revisions, checksums, and 50
passing checks per platform. Linux, Intel Mac, signing/notarization, hosted CI,
and the standing accessibility gaps remain unverified as listed in Feature status.

## 2026-09-22: rebase design preparation

A [detailed design](REBASE_DESIGN.md) now describes the proposed first rebase
workflow, safety checks, stopped-state recovery, and acceptance gate. The user
confirmed local or fetched remote branches as selectable targets, a linear
source range, and a separately reviewed Skip that warns the commit's change
will be omitted. Disposable local repositories using macOS
Git 2.51.2 confirmed conflict-stop metadata and staged-resolution Continue,
showed that Continue can drop a commit after an `--empty=stop` pause, and
accepted the proposed non-interactive command flags for a clean two-commit
replay. The clean probe reattached the feature branch with the target tip as
the first replayed commit's parent. This preparation does not validate the app
implementation or Windows behavior; the latest app acceptance remains A4 at
`9fe0fb8` below.

## 2026-09-22: A4 complete text-hunk staging delivered on tested platforms

Source checkpoint `9fe0fb8194723e9253c9e8782c443d6c3e4f2e0f` was synchronized
to the designated Windows checkout without publishing. The A4 macOS and Windows
gate passes. In **Working changes**, users can select a supported tracked text
file and Stage hunk or Unstage hunk from the respective diff. Backend identities
bind the reviewed complete block to the exact file/index/working/configuration
snapshot. Git updates the index only; stale selections are rejected, and
unexpected post-hook changes are reported for inspection. Whole-file controls
remain available for supported files outside the hunk scope.

| Gate | macOS arm64 | Windows 11 x64 |
| --- | --- | --- |
| Frontend | 111 passed; typecheck/lint passed | 111 passed; typecheck/lint passed |
| Rust | 163 passed; one opt-in performance case ignored | 155 passed; one opt-in performance case ignored |
| Formatting and strict Clippy | Passed | Passed |
| Native desktop | 46/46 checks passed | 46/46 checks passed |
| Release package | `.app` built; packaged CLI/startup passed | NSIS built; installed CLI/native window passed |

Commands: `pnpm test`, `pnpm typecheck`, `pnpm lint`, locked Cargo tests,
`cargo fmt --check`, strict all-target Clippy with `native-smoke`, the
`VITE_NATIVE_SMOKE=1 pnpm build` frontend build, a custom-protocol native
build, and `python3 scripts/native-smoke.py` with report path
`/tmp/git-view-native-a4-mac-final.json` on macOS. Windows used
`scripts/windows-sync.py`, `scripts/windows-remote.py scripts/windows-verify.ps1`,
`scripts/windows-release.ps1`, and `scripts/windows-installer-smoke.ps1`.
The focused macOS `git_hunks` suite and native checks were rerun after the
Windows fixture correction. The final macOS release build and packaged
help/version/invalid-path/repository/discovery startup checks passed.

Passing reports: `artifacts/native-macos.json` and `artifacts/native-windows.json`
(Windows original `C:\codeme\git-view\.tools\GitViewNativeSmoke-a1fe5870d527446f8631fdc330c42c4d.json`).
The Windows installed test package is at
`C:\codeme\git-view\.tools\installed-smoke-f0e062da82fc45e9970d441ed0c88885`.
Both final packages are unsigned and were collected locally, not published:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `artifacts/Git-View_0.1.0_macos-arm64.zip` | 3,724,715 | `5a3f3d7185837447968f04c6c915c516d57d43391adf438f02fb400077ac3fa1` |
| `artifacts/Git-View_0.1.0_windows-x64-setup.exe` | 2,952,956 | `8dc51e216e92393cc4b3b55efa04dd755020b517a0e4e21f02c5b43f831479fe` |

`artifacts/manifest.json` records matching source revisions, checksums, and
46 passing checks per platform. Prior A3 artifacts remain in
`artifacts/checkpoints/7c0eef5`. Linux, Intel Mac, signing/notarization,
remote-service authentication, and the outstanding accessibility checks remain
unverified as listed in Feature status. The next planned feature stage is rebase;
it requires a separate detailed design before implementation.

### A4 implementation checkpoints

At the initial A4 checkpoint, macOS source checks passed in the worktree over
`7c0eef5`:
`pnpm typecheck`, `pnpm lint`, `pnpm test` (111 passed), locked full Cargo tests
(163 passed; one opt-in performance case ignored), `cargo fmt --check`, and
strict all-target Clippy with `native-smoke`. `VITE_NATIVE_SMOKE=1 pnpm build`
and the custom-protocol native build pass. The native smoke report at
`/tmp/git-view-native-a4-mac.json` passes **46/46 checks**, including complete
first/second hunk stage and unstage, exact intermediate index/working bytes,
staged diff review, retained draft, and a partial commit leaving the other
edit visible. Windows and local release/package gates were still open at that
checkpoint; the current completed gate is recorded above.

The first Windows full run at `9ba61b0` passed 111 UI tests, typecheck/lint,
then stopped at the A4 Unicode/line-ending fixture because Windows rejects
a tab in a filename (OS error 123) before Git executes. The fixture now uses
a Unicode path with spaces on Windows and retains the tab-path case on macOS.
The focused macOS hunk suite and formatting pass after this test-only change;
Windows full rerun remains required. The first Windows run is not a passing gate.

A3's full gates passed before A4 started. Shared typed hunk capabilities,
backend/index safety, renderer actions, and native fixtures are implemented.
The A4 platform/package gate was still in progress then. A3 packages/reports are preserved at
`artifacts/checkpoints/7c0eef5` and remain the current collected artifact set.

## 2026-09-22: A3 stash complete on tested platforms

Acceptance and local packages are at `7c0eef5dee315cb582ad7f1a634933ad7ab19658`
on macOS and the designated Windows checkout. Named Create, saved-section
inspection, retained Apply with explicit index restoration, exact-occurrence
Delete with backup, and reviewed interrupted-deletion recovery are delivered.

| Check | macOS arm64 | Windows 11 x64 |
| --- | --- | --- |
| Frontend tests | 98 passed | 98 passed |
| Rust tests | 155 passed | 147 passed |
| Native DOM/IPC checks | 43 passed | 43 passed |
| Typecheck, lint, formatting, strict native-feature all-target Clippy | Passed | Passed |
| Package | `.app` built; packaged CLI/startup passed | NSIS built; installed CLI/native window passed |

Full Rust runs used `9fb0912`. Exact-entry identity changed afterward at
`6403443`, with its linked-worktree/duplicate/stale regression rerun on macOS and
the complete stash integration suite rerun on Windows. Final `7c0eef5` changes
only the native fixture's hook path and error diagnostics plus documentation;
both full native suites and release/package gates passed there. The existing
opt-in backend timing test remains ignored. The 18 new macOS backend tests
(16 applicable on Windows) include durable interruption phases, SHA-256 recovery,
prepared hooks, packed refs, native whole-reflog deletion, duplicate entries,
linked worktrees, ignored content, staged-state restoration, and submodule guards.

Commands include `pnpm test`, `pnpm typecheck`, `pnpm lint`, locked Cargo tests,
formatting, strict all-target Clippy with `native-smoke`, native frontend and
custom-protocol builds, `scripts/native-smoke.py`, `pnpm tauri build --bundles app`,
and `scripts/release-smoke.py`. Windows used `scripts/windows-sync.py`,
`windows-verify.ps1`, the focused `git_stash` integration suite,
`windows-native-verify.ps1`, `windows-release.ps1`, and
`windows-installer-smoke.ps1`. The five new native stash workflows pass alongside
all prior group, merge, both squash, discovery, search, and performance checks.
Documentation review checked 100 local links/anchors and `git diff --check`.

Collected unsigned packages and matching reports in `artifacts/manifest.json`:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `artifacts/Git-View_0.1.0_macos-arm64.zip` | 3,671,080 | `4f17194f4da3cd3974f6c9b023f617a55ad4aa439b294d4d15233617f164d6c2` |
| `artifacts/Git-View_0.1.0_windows-x64-setup.exe` | 2,896,867 | `c5ffb482483d90bae9724b47527b4635e2830e169cc7874c22ea66fe6808328a` |

Native reports: `/tmp/git-view-native-a3-mac.json` and
`C:\codeme\git-view\.tools\GitViewNativeSmoke-eb1dc12ebde94fa1a5f7b7a2dc01429a.json`.
Installed Windows test package:
`C:\codeme\git-view\.tools\installed-smoke-142b28b2dfaa41d38463509d88ec2289`.
A2 artifacts remain archived at `artifacts/checkpoints/fd637b8`.
Stash requires files ref storage; safe Delete is macOS/Windows-only. The guide
records limits, submodule exclusions, partial Apply failure, and retained backups.
Remaining platform/accessibility/authentication/signing gaps remain in Feature
status. Nothing was pushed or published. A4 hunk staging is next.

## 2026-09-22: A3 stash implementation in progress

Windows final native follow-up at `6403443` passes 41 checks, including the four
normal stash workflows. Its delayed-Create reload fixture used a verbatim
Windows path as `core.hooksPath`; a disposable probe reproduced Git exit 128 for
that path and success for a repository-relative path. The native-only fixture
now uses `.git/reload-stash-hooks` and includes full backend error diagnostics.
This requires the remaining native rerun; it does not change production actions.

macOS acceptance at `6403443`: **98 frontend tests and 43 native DOM/IPC checks
pass**, including all five new stash workflows. The release `.app` builds and
packaged help/version/invalid-argument plus repository/discovery startup checks
pass. `cargo fmt`, strict native-feature Clippy, 100 local documentation
links/anchors, and `git diff --check` pass. Windows passed the full **147-test**
Rust suite at `9fb0912`; the stable-entry identity follow-up at `6403443` still
needs its Windows focused/backend and native/package gates. Current collected
artifacts remain A2 until both A3 packages and reports are ready.

Integration follow-up at `9fb0912`: the full macOS Rust suite passes **155 tests**.
The first Windows run exposed a read-only replacement-file flush (`os error 5`);
the corrected writable-handle implementation passes all 24 Windows library tests,
including interrupted Delete and concurrent whole-reflog removal. The full
Windows run is still underway. Native macOS reached 38 checks but caught a stash
selection disappearing during a delayed operation refresh; stable exact-entry
identity and renderer/harness regression fixes are in progress. An earlier native
attempt launched a normal binary after a concurrent Cargo test build replaced
the smoke executable; it timed out and is not passing evidence. Native builds
and launches are now sequenced after ordinary test builds.

A2 is complete at `fd637b8`. A3 shared contracts, command registration, renderer
API, stash UI, and native fixture/verifier scaffolding are in place. The frontend
suite passes **96 tests**, including 13 stash cases; typecheck/lint pass, and
`cargo check --manifest-path src-tauri/Cargo.toml --locked --features native-smoke`
passes. Backend interruption/concurrency tests remain underway; A3 has no passing
platform gate yet. The macOS release app builds; package launch validation is
pending. The full macOS Rust run passed 152 tests before three final adversarial
stash tests were added; the focused stash suites now pass 18 tests, and strict
native-feature Clippy passes. Fixtures cover
unchecked untracked inclusion, opt-in creation and staged-state restoration,
stash conflicts without ordinary merge controls, exact middle-entry Delete,
and an actual delayed Create operation surviving a WebView reload.

Disposable preparation probes confirmed that Apply can overwrite ignored paths
or leave partial changes on failure, default Apply may leave new files staged,
and nested submodule edits are not saved. Delete requires exact reflog occurrence
identity rather than a list number or object ID alone. The design uses Git's ref
lock, conditional atomic reflog replacement, and explicit journal/backup recovery;
whole-reflog deletion can bypass that ref lock, so a missing destination must not
be silently recreated. These are implementation requirements pending regression
and platform validation, not evidence of delivered A3 functionality.

A2 packages, manifest, and native reports were preserved under
`artifacts/checkpoints/fd637b8` before preparing the next artifact collection.

## 2026-09-22: A2 cherry-pick complete on tested platforms

Delivered at `fd637b801dfae53b6f3efb74d37bf8ffd56fab89` on both build hosts.
Reviewed single non-merge cherry-pick, Continue/Abort/empty Skip, persistent
source/target conflict guidance, safe interrupted-marker cleanup, ignored-path
protection, and nonrecursive recovery are implemented. Unknown or conflicting
operation records remain inspectable and block unsafe mutations.

| Check | macOS arm64 | Windows 11 x64 |
| --- | --- | --- |
| Frontend tests | 83 passed | 83 passed |
| Rust tests | 137 passed | 131 passed |
| Native DOM/IPC checks | 38 passed | 38 passed |
| Typecheck, lint, formatting, strict all-target native-test Clippy | Passed | Passed |
| Release package | `.app` built; packaged CLI/startup passed | NSIS built; installed CLI/native window passed |

Commands: `pnpm test`, `pnpm typecheck`, `pnpm lint`, locked Cargo tests,
`cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`, strict
all-target Clippy with `native-smoke`, native frontend/custom-protocol builds,
`python3 scripts/native-smoke.py /tmp/git-view-native-a2-mac.json`,
`pnpm tauri build --bundles app`, and `scripts/release-smoke.py` against the bundle.
Windows used `scripts/windows-sync.py` and `scripts/windows-remote.py` with
`windows-verify.ps1`, `windows-release.ps1`, and `windows-installer-smoke.ps1`,
plus separate native-feature strict Clippy/formatting. Installed test location:
`C:\codeme\git-view\.tools\installed-smoke-04e5eab129d34be0adca2ddb2951c4c5`.

The 15 new backend tests cover source metadata/parent, conflicts, hooks/signing,
initially/becomes-empty states, external changes, ignored file/directory hazards,
submodule bytes, and interrupted/unknown context. Four new native workflows cover
success, modify/delete Abort, empty Skip, and add/add resolution after a real
WebView reload. Existing group, merge, both squash, discovery, search, and native
performance checks passed. The existing opt-in backend timing test remains ignored.
Documentation review checked 98 local file links/anchors and `git diff --check`.

Collected unsigned packages, matching reports, and `artifacts/manifest.json`:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| `artifacts/Git-View_0.1.0_macos-arm64.zip` | 3,533,278 | `9849a8e3f519fafe2c738f3ee067708fae78f7ff8813888ab38a1ce6a3dd2d29` |
| `artifacts/Git-View_0.1.0_windows-x64-setup.exe` | 2,809,032 | `e77275313d76dbfb2eb6cd6f41acd1d25f461d0ccbf2a9f3ab71b1d841c37c44` |

Windows native report:
`C:\codeme\git-view\.tools\GitViewNativeSmoke-0ab37703d8a64e559dc878d8e26705c0.json`.
A1 artifacts/reports were preserved at `artifacts/checkpoints/fe33bf3` before
collection. Linux/Intel Mac, hosted CI, signing/notarization, real remote-service
authentication, full accessibility/long-group navigation, and the native OS save
picker remain unverified as listed in Feature status. No push or publication.
A3 stash is next; A4 hunk staging remains planned.

## 2026-09-22: A2 cherry-pick and conflict guidance in progress

A1 is complete at `fe33bf3`; A2 changes are under implementation and have not
passed their platform gates yet. Shared operation context and reviewed single
commit cherry-pick/continue/abort/skip contracts are being integrated. Native
fixtures cover success, add/add conflict continuation across WebView reload,
modify/delete conflict abort, and an already-applied commit with explicit Skip.
Disposable Git probes found ignored-file overwrite hazards (including directory
rename destinations) and different native hook behavior between initial
cherry-pick and Continue; backend safeguards and regression tests are required.
No A2 release artifact or acceptance completion is claimed by the A1 evidence below.

Interim frontend gate: `pnpm test` passes **79 tests** after adding 12 cherry-pick
component regressions; typecheck and lint pass. Review corrected unknown external
target wording, marker-only interrupted-pick recovery, and the active-operation
guard. Native fixtures/verifiers were cross-reviewed, but execution is pending
backend tests. A further disposable probe reproduced recursive submodule edits
being discarded by Git Abort; explicit protection is being implemented before
the A2 native gate.

Final local source checks now pass: **83 frontend tests** (`pnpm test`), **137
Rust tests** (`cargo test --manifest-path src-tauri/Cargo.toml --locked`),
including 15 cherry-pick fixtures, plus typecheck, lint, formatting, and strict
all-target Clippy with `native-smoke`. The existing opt-in backend performance
measurement remains ignored; native performance regression coverage still runs.
Final fixes protect submodule bytes, retain unknown source context, and block
cross-operation mutations when a stale cherry-pick journal needs manual review.
The macOS native build succeeded and **38 native DOM/IPC checks passed** via
`python3 scripts/native-smoke.py /tmp/git-view-native-a2-mac.json`. This includes
source metadata/parent validation, modify/delete Abort, explicit empty Skip,
and add/add resolution after an actual WebView reload. Windows and release/package
checks are pending at this checkpoint.

## 2026-09-22: A1 remote tracking checkout complete on tested platforms

Delivered at `fe33bf3f9183a65f3bc0c0fb18fd6c2e9d98e0e8` on both build hosts.
Explicit remote/source selection, editable local name, exact upstream preview,
collision guidance, mapping validation, source locking, and post-hook state
inspection are implemented. Scope selection remains read-only. Unsupported or
ambiguous fetch mappings are explained; no force/reset/automatic rollback occurs.

| Check | macOS arm64 | Windows 11 x64 |
| --- | --- | --- |
| Frontend tests | 67 passed | 67 passed |
| Rust tests | 122 passed | 116 passed |
| Native DOM/IPC checks | 34 passed | 34 passed |
| Typecheck, lint, strict all-target native-test Clippy | Passed | Passed |
| Release package | `.app` built; packaged CLI/startup passed | NSIS built; installed CLI/native window passed |

Mac commands: `pnpm test`, `pnpm typecheck`, `pnpm lint`,
`cargo test --manifest-path src-tauri/Cargo.toml --locked`,
`cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`,
`cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets --features native-smoke -- -D warnings`,
`VITE_NATIVE_SMOKE=1 pnpm build`, native-smoke/custom-protocol Cargo build,
`python3 scripts/native-smoke.py /tmp/git-view-native-a1-mac.json`,
`pnpm tauri build --bundles app`, and `scripts/release-smoke.py` against the bundle
executable. GUI checks required desktop access outside the sandbox. The new
harness's asynchronous second-preview wait was corrected before the passing run.

Windows commands: `scripts/windows-sync.py`, then `scripts/windows-remote.py`
with `windows-verify.ps1`, `windows-release.ps1`, and
`windows-installer-smoke.ps1`; strict Clippy with `native-smoke` was also run
separately. Installer test location:
`C:\codeme\git-view\.tools\installed-smoke-3b51b640bff04a159045c571638fda6f`.
Native report:
`C:\codeme\git-view\.tools\GitViewNativeSmoke-a06c80ca051645f9b8aaaa8a4a9f8ffa.json`.

Collected unsigned artifacts (supersede prior files at these paths):

- [macOS arm64 archive](../artifacts/Git-View_0.1.0_macos-arm64.zip): 3,473,379 bytes;
  SHA-256 `f13ea6a79ca6ce7c575479bacdc5c2ad3aee3392ca51f49135f4532a710655ab`.
- [Windows x64 installer](../artifacts/Git-View_0.1.0_windows-x64-setup.exe): 2,746,669 bytes;
  SHA-256 `f0fb1cd5ee78b14d35a8230f01a3dd1e7d459c0ca50112d06e2e8c5f04a92468`.
- [Manifest](../artifacts/manifest.json), [Mac native report](../artifacts/native-macos.json),
  and [Windows native report](../artifacts/native-windows.json).

Documentation links/anchors and whitespace checks pass. Guide, inventory,
contracts, and acceptance instructions describe the delivered workflow. Existing
unrelated README/runbook/setup edits remain preserved; no push or publication.
Linux/Intel Mac, signing/notarization, and earlier accessibility/authentication
gaps remain unverified. A1's required local delivery gate is complete; next is A2
cherry-pick and conflict guidance, before stash and hunk staging.

## 2026-09-22: A1 remote tracking checkout resumed and integrated

Recovered unfinished worker edits after the computer crash and preserved existing
documentation/setup changes. Integrated backend candidate mapping and guarded
checkout with typed IPC, repository UI, 15 real-Git fixtures, 10 renderer tests,
and native success/dirty/collision fixtures. User guide, feature inventory,
contracts, Git semantics, and acceptance documentation updated.

Current Mac checks: `cargo test --manifest-path src-tauri/Cargo.toml --locked`
passes **122 tests**, with one existing performance test ignored; `pnpm test`
passes **67 tests**. `pnpm typecheck`, `pnpm lint`, Rust formatting, and strict
all-target Clippy with `native-smoke` pass. Native-test app compilation passes.
The first sandboxed GUI run timed out without desktop access. Rerunning with
desktop access passed the first 30 checks, then exposed a timing error in the new
test harness: it queried the second confirmation dialog before the asynchronous
preview finished. The corrected harness and complete native rerun pass all **34 checks**.
Retained Mac report: `artifacts/native-macos.json`.

Windows host is reachable with Rust 1.98.1. Source checkpoint
`fe33bf3f9183a65f3bc0c0fb18fd6c2e9d98e0e8` is synchronized to the clean build
checkout. Windows passes **116 Rust tests**, **67 frontend tests**, typecheck/lint and
strict Clippy. Windows native acceptance and release packaging remain in progress.
Mac `pnpm tauri build --bundles app` passes, and `scripts/release-smoke.py` passes
CLI help/version/invalid arguments plus repository/discovery startup against the
packaged executable.
No A1 release/complete-platform claim yet. A2–A4 have not started.

## 2026-09-22: next-stage product scope finalized

Confirmed rebase for the following stage, after remote tracking checkout,
cherry-pick, stash, and hunk staging. Confirmed the first conflict improvement:
clear branch/commit context, conflict list, and next-step guidance with external
file resolution. The integrated three-way editor is deferred.

Finalized [Next feature stage](NEXT_STAGE.md) with A1–A4 delivery gates, shared
conflict-guidance work in A2 extended for stash in A3, representative 15-repository
usage, and documented engineering assumptions. Updated decision/roadmap links;
no further product answers are needed to begin A1. Implementation has not started.
Planning-only validation: local Markdown links and whitespace, including
`git diff --check`; no application tests/builds rerun. Current user-guide behavior
and completed-feature claims are unchanged. Existing edits preserved; nothing
pushed or published.

## 2026-09-21: visual workflow purpose clarified

User clarified they use both terminal and visual tools for simple Git operations;
complex merges, rebase, and unclear state are the reason to seek a visual client.
Recorded this in the product/decision/next-stage documents and added acceptance
goals for source/target clarity, operation-specific stopped states, conflict
progress, recovery effects, and refresh/reload reconciliation. No existing
push/pull/merge defect was reported by this clarification. Asked about rebase
timing and external versus integrated conflict resolution; answers pending.
Planning only: documentation link/whitespace checks, no application tests/builds.

## 2026-09-21: cherry-pick added to the confirmed stage

User confirmed the revised order: remote tracking checkout → cherry-pick → stash
→ hunk staging, retaining the small-slice delivery gate. Renumbered the planning
slices A1–A4 and expanded cherry-pick's proposed single-commit flow, distinct
conflict/continue/abort states, empty-pick handling requirement, and acceptance
coverage. Existing push/pull/merge friction is the remaining open workflow
question. Application behavior is unchanged; checks for this update are planning
document links and `git diff --check`, not application test/build reruns.

## 2026-09-21: workflow context and stash preferences recorded

User identified push/pull/merge/cherry-pick as tasks that send them outside the
app; typical groups contain up to 15 repositories with common branch names and
`main` as the default. Confirmed stash Apply retains the entry, Delete is separate
and reviewed, and Include untracked files starts unchecked. Expanded A2 planning
and its acceptance criteria accordingly. Asked whether cherry-pick should move
after checkout and what causes friction in the existing push/pull/merge workflows;
those answers remain pending. Updated planning/decision records only; no feature
implementation, application tests, or builds performed in this planning update.

## 2026-09-21: everyday Git stage selected

User confirmed remote tracking checkout → stash → hunk staging, delivered in
small slices that are tested and usable before moving on. Updated
[Next feature stage](NEXT_STAGE.md) and [Decisions](DECISIONS.md) to record both
answers. Expanded A1 with the proposed interaction, action/metadata boundaries,
ownership, failure behavior, and backend/renderer/native acceptance cases.
Stash restore actions, untracked-file defaults, and workflow examples remain
pending. Planning only; application behavior, user-guide instructions, and
implementation status are unchanged. No application tests or builds rerun.

## 2026-09-21: next-stage planning started

Added [Next feature stage](NEXT_STAGE.md), a draft covering everyday Git,
group operations, and change-inspection tracks, with proposed boundaries,
acceptance cases, implementation dependencies, and the existing Mac/Windows
delivery gate. Asked the user for the leading track, delivery cadence, and
current workflow pain points/group scale. Answers remain pending; no proposed
feature is marked implemented and no application implementation began.

Reviewed current roadmap, guide, acceptance requirements, action contracts,
and backend preview/state checks. Documentation links and whitespace were
checked; application tests/builds were not rerun for this planning-only change.
The user guide's current behavior remains unchanged. Existing edits are
preserved; nothing pushed or published.

## 2026-09-20: user guide and roadmap inventory

Reviewed the current application at `e044bcbc` against `docs/PRODUCT.md` and the
overnight runbook. Added [User guide](USER_GUIDE.md) with discovery/group setup,
history/search, staging/commits, remote actions, branches, both squash workflows,
recovery, shortcuts, and troubleshooting. Added [Feature status](FEATURE_STATUS.md)
to distinguish the completed runbook workflows from the broader unfinished
product roadmap and outstanding platform/acceptance checks.

Added repository-wide maintenance instructions in [AGENTS.md](../AGENTS.md) and
a [pull request checklist](../.github/pull_request_template.md): future user-facing
changes must update the guide and feature inventory in the same change. Linked
both from README and clarified planned versus implemented persistence in the
project-file specification. These instructions/checklists are a contributor
requirement, not automatic prose generation or an enforced CI freshness check.

Validation: compared guide controls, CLI forms, operation preconditions, and
recovery limits with the implementation; local Markdown links/anchors and new
document whitespace/code fences passed checks; `git diff --check` passed.
Documentation only: no application code or workflow behavior changed and no
application tests or native builds were rerun. The tested revisions, results,
and packages below remain the latest application evidence. Existing README,
runbook, build-status, and setup-document changes were preserved. Nothing pushed
or published.

## 2026-09-20: scoped history search complete on tested platforms

Final checkpoint: `e044bcbcaa1ba0a4f939ce98b47bb80ac4d420b8` on both
build hosts. Application source is unchanged from `f5526ef`; the final checkpoint
only documents a Windows fixture permission attribute and strengthens the focused
verification script's Clippy gate.

Implemented next roadmap slice: search commit messages, author names/emails, or
commit-ID prefixes within the chosen history scope. Results retain immutable
snapshot paging and real parent metadata, and display without graph edges across
omitted commits. Search remains read-only. UI supports explicit submit/clear,
Cmd/Ctrl+F, scoped Escape, retained drafts, and stale-response rejection.

Mac verification passes 57 frontend tests, 107 Rust tests, strict native-smoke
Clippy, and all 31 native workflow checks. The 11 new Git fixtures cover literal
case/Unicode/body/email searches, scoped ambiguous OID prefixes, immutable paging
beyond 200 results, concurrent cursor preservation, actual parent relationships,
shallow drift, missing objects, replacements, and unchanged index/journal bytes.
The eight renderer regressions cover query/scope/paging/keyboard/error races.
Native checks search real commits, inspect a matching file diff, and restore the
normal graph after clearing. Mac normal release build succeeds.
Windows passes all 101 Rust tests and 57 frontend tests. Its first strict Clippy
run flagged Windows-only read-only-attribute clearing in a disposable missing-object
fixture. A statement-scoped allowance documents that no Unix permission bits are
changed; application code is unchanged. Final focused Windows strict Clippy and
all **31 native checks pass** at the final checkpoint. The existing group,
discovery, merge, both squash, recovery, and reload workflows remain covered.
Packaged Mac CLI help/version/invalid-argument checks and separate repository/
discovery startup checks pass with desktop access. Native report:
`/tmp/git-view-native-search-mac.json`, retained as `artifacts/native-macos.json`.

Executed Mac commands: `pnpm typecheck`, `pnpm lint`, `pnpm test`,
`cargo test --manifest-path src-tauri/Cargo.toml --locked`,
`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --features native-smoke -- -D warnings`,
`cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`,
`VITE_NATIVE_SMOKE=1 pnpm build`,
`cargo build --manifest-path src-tauri/Cargo.toml --features native-smoke,tauri/custom-protocol`,
`python3 scripts/native-smoke.py /tmp/git-view-native-search-mac.json`,
`pnpm tauri build --bundles app`, and `python3 scripts/release-smoke.py`
against the packaged app executable.

Review fixes: cursors retain concurrent issued continuations; search and normal
history resolve HEAD without status cleanup; historical metadata/file lists/diffs
all ignore replacement refs consistently; shallow/graft changes expire a page.
Queries are limited to 512 UTF-8 bytes and 30 seconds; overly broad OID prefixes
request refinement. Unsubmitted Clear and dismissed-error states no longer leave
loading stuck or falsely claim zero matches. Two new fixture setup mistakes were
corrected before the full successful backend rerun.

| Final verification | macOS arm64 | Windows 11 x64 |
| --- | --- | --- |
| Frontend unit/component/graph tests | 57 passed | 57 passed |
| Rust unit/integration/acceptance tests | 107 passed | 101 passed |
| Native DOM/IPC workflow checks | 31 passed | 31 passed |
| Typecheck, lint, strict native-test Clippy | Passed | Passed |
| Normal release build | `.app` passed | NSIS installer passed |
| Packaged check | CLI/invalid arguments/startup passed | Silent installation, CLI and titled native window passed |

Windows verification used `python3 scripts/windows-remote.py` with
`scripts/windows-verify.ps1`, `scripts/windows-native-verify.ps1` after the
test-only lint correction, `scripts/windows-release.ps1`, and
`scripts/windows-installer-smoke.ps1`. Installed test application retained at
`C:\codeme\git-view\.tools\installed-smoke-5a63e172a8da42e6afc8d1cfef86cdfe`.
Native report:
`C:\codeme\git-view\.tools\GitViewNativeSmoke-3cde1520a1d34d38b6bd7cd82eee7ef4.json`.

Current unsigned artifacts (replace earlier artifacts at these paths):

- [macOS arm64 archive](../artifacts/Git-View_0.1.0_macos-arm64.zip), 3,402,017 bytes;
  SHA-256 `25867c840aab4cfe90018b7d1bf3007b8fdd66aaa58a9e4c30cc16f90ff95791`.
- [Windows x64 installer](../artifacts/Git-View_0.1.0_windows-x64-setup.exe), 2,693,915 bytes;
  SHA-256 `b5bad7609057c5086e8e49c9334c4d67244c928e6848fe8d6750d831cf9fbefe`.
- [Source/hash manifest](../artifacts/manifest.json),
  [Mac native report](../artifacts/native-macos.json),
  [Windows native report](../artifacts/native-windows.json).

Debug native 10,000-commit fixture creation plus first-page navigation measured
352.0 ms on Mac and 2,116.4 ms on Windows; scrolling plus two frames measured
20.0 ms and 3.0 ms. The Windows sample overlapped the release compiler and includes
fixture creation, so it is not an isolated history-page latency measurement.
Warm CLI welcome/render measured 181.0 ms and 599.9 ms respectively.
The normally ignored backend performance check was then run explicitly without
a concurrent release compiler: first 200 commits from 10,000 took **44.37 ms Mac /
190.15 ms Windows**; 25 sequential status reads took **187.90 ms / 1,425.20 ms**.
Both measurements passed (one additional measurement test per platform, separate
from the regular test counts above). Command: `cargo test --manifest-path
src-tauri/Cargo.toml --test git_engine acceptance_performance_measurements --
--ignored --nocapture`; Windows wrapper: `scripts/windows-performance.ps1`.

Implementation and test fixes are checkpointed locally. Existing README/runbook/
setup-document changes remain preserved; final progress documentation remains
uncommitted. Nothing pushed or published. Builds remain unsigned/not notarized;
Linux and Intel Mac execution remain unverified. History search intentionally
ignores replacement refs and rejects legacy grafts, has bounded queries, and
does not offer arbitrary revision expressions. No known blocker remains for this
implemented slice. Earlier sections retain prior checkpoint evidence.

## 2026-09-20: folder discovery and easier group creation complete

Implemented after completion of M1–M3 below. Adds bounded read-only
repository discovery, `group --scan` with depth control, explicit selection/save,
and reviewed additions to existing project drafts. The original three workers
retain Git-engine, groups, and repository/native-harness ownership; coordinator
owns contracts, CLI routing, navigation, and platform validation.

Final source checkpoint: `1593da8967cc239a9872f2003387019563cfad00` (Mac and Windows).
User confirmed folder discovery/easier group creation as the next priority.

- CLI: `git-view group --scan <folder> [--depth <0..10>]`; default 3, root 0.
- GUI: choose a root, scan, review unchecked candidates, select a subset, then
  save a named project. Existing-group discovery adds selected candidates to a
  draft; rescanning/canceling never silently changes saved membership.
- Scanner recognizes linked worktrees, skips dependency/build directories and
  symlinks/reparse points, reports bounded partial results, and never refreshes
  the index or cleans up squash journals. Unsupported non-UTF-8 paths are omitted
  with an explicit issue instead of becoming lossy JSON action paths.
- Navigation waits for native dialogs and pending project creation, rechecks
  asynchronous launch results, and preserves whitespace in selected folder paths.

Mac: 49 frontend tests, 96 Rust tests, typecheck/lint/formatting, and strict
native-smoke Clippy pass. Native DOM/IPC acceptance passes all **29 checks**,
including the previous merge/squash/combine/reload checks and three discovery
workflows. Report: `/tmp/git-view-native-discovery-mac.json`, retained in
`artifacts/native-macos.json`. Updated normal `.app` build succeeds.
Windows backend gate passed 90 tests and strict Clippy at `8e373b5`; subsequent
changes affect only native-harness assertions and a focused verification script.
Windows junction escape/cycle coverage passes. Final frontend gate passes 49
tests, typecheck and lint; native acceptance passes all **29 checks**. Report:
`C:\codeme\git-view\.tools\GitViewNativeSmoke-e79cad9c3671451db66a036900393311.json`,
retained as `artifacts/native-windows.json`. Normal Windows NSIS build and silent
installation pass; installed CLI help/version and the actual titled Git View
window pass at `.tools\installed-smoke-ecfe8b2c9286427ca98a9a4fbc446ce8`.

Mac release checks pass help/version, invalid root/depth rejection, and separate
repository/discovery process startup; discovery writes no project automatically.
One sandboxed launch aborted in macOS `_RegisterApplication`; rerunning the same
packaged binary with approved desktop access passed both startup checks.

| Final verification | macOS arm64 | Windows 11 x64 |
| --- | --- | --- |
| Frontend tests | 49 passed | 49 passed |
| Rust unit/integration/acceptance tests | 96 passed | 90 passed |
| Native DOM/IPC checks | 29 passed | 29 passed |
| Release build and packaged startup | Passed | Passed, including installation |

Unsigned local artifacts at the final source checkpoint:

- [macOS archive](../artifacts/Git-View_0.1.0_macos-arm64.zip), 3,360,067 bytes;
  SHA-256 `9c6db18ebe5881519e4cc629de247ef25dc5e9a03925e3307ab00c0244159b0b`.
- [Windows installer](../artifacts/Git-View_0.1.0_windows-x64-setup.exe), 2,655,357 bytes;
  SHA-256 `e07a55f7404479b61d3aff34ae5c96e15822b08a40e08883f00a16c655ccbef3`.
- [Source/hash manifest](../artifacts/manifest.json),
  [Mac native report](../artifacts/native-macos.json),
  [Windows native report](../artifacts/native-windows.json).

Existing README/runbook/setup-document edits remain preserved. Final status and
acceptance documentation edits remain uncommitted; implementation is checkpointed
locally. Nothing pushed or published. Linux/Intel Mac execution and signing remain
outside the verified platform matrix. This discovery slice has no known blocker.

Commands: `pnpm typecheck`, `pnpm lint`, `pnpm test`,
`cargo test --manifest-path src-tauri/Cargo.toml --locked`,
`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --features native-smoke -- -D warnings`,
`VITE_NATIVE_SMOKE=1 pnpm build`,
`cargo build --manifest-path src-tauri/Cargo.toml --features native-smoke,tauri/custom-protocol`,
`python3 scripts/native-smoke.py /tmp/git-view-native-discovery-mac.json`,
and `pnpm tauri build --bundles app`.
Windows commands use `python3 scripts/windows-remote.py` with
`scripts/windows-verify.ps1`, then `scripts/windows-native-verify.ps1` after
harness corrections, `scripts/windows-release.ps1`, and
`scripts/windows-installer-smoke.ps1`. `scripts/collect-artifacts.py` checks
matching source revisions and collects both passed reports and release packages.

Final debug native timings (disposable 10,000-commit fixture): Mac first-page
fixture/navigation 444.0 ms, scroll plus two frames 21.0 ms; Windows 1,137.4 ms
and 2.7 ms. Warm welcome forwarding/render was 588.0 ms on Mac and 372.5 ms
on Windows. These are individual acceptance samples, not production benchmarks.

Review fixes and failed checks resolved: discovery originally reused status
loading, which could remove a completed squash journal; it now shares only
read-only identity registration, with byte-preservation regression coverage.
Native CLI assertions now accept the equivalent trailing `/.` path spelling.
A permission dialog stole native window focus during one run; the test harness
now restores actual native focus before retaining the same visible-outline
assertion. Folder whitespace and navigation during project saves have regressions.
Windows' saved display paths may mix slash styles; native save/reopen assertions
now compare canonical repository IDs and exact member counts instead of display
spelling. `scripts/windows-native-verify.ps1` reruns frontend checks and native
acceptance after harness-only changes without repeating the unchanged Rust suite.

Native automation selects real DOM candidates and uses real project-save IPC
with a fixture destination, then reopens via CLI and tests actual DOM draft/save
controls. The operating-system save picker itself is not automated; its call
and selection behavior are covered by explicitly mocked component tests.
The Mac filesystem rejects creation of raw-invalid-UTF-8 names, so that fixture
reports the platform limitation rather than claiming an on-disk raw-name check.

## Prior checkpoint — M1, M2 and M3 complete on tested platforms

Application/release source: `6a28ef976bd92f8baf844bdc04f91a19764dd3b9`.
Completed the runbook tasks F1–F2, B1–B4, P1–P3, G1–G3, R1–R3 and Q1,
then M2 ordinary merge/squash-merge and M3 Combine commits with durable recovery.
One coordinator and the three original workers kept the documented ownership.
Prerequisites are resolved: rustup is available, Windows pnpm/PATH and native
C++ linker setup work, and both native webviews have executed real workflows.

| Verification | macOS arm64 | Windows 11 x64 |
| --- | --- | --- |
| Frontend unit/component/graph tests | 34 passed | 34 passed |
| Rust unit/integration/acceptance tests | 83 passed | 79 passed |
| Native DOM/IPC workflow checks | 26 passed | 26 passed |
| Typecheck, lint, strict Clippy | Passed | Passed |
| Release build | `.app` passed | NSIS installer passed |
| Packaged application check | CLI and process startup passed | Installed CLI and titled native window passed |

The platform count difference is from Unix-specific checks. One performance
test is ignored by default and was run explicitly on both platforms; timings
and test details are recorded below. Rust formatting and final diff checks pass.
Tests use disposable repositories/local remotes; normal release builds exclude
the native-smoke instrumentation.

Prior artifact measurements (the same paths now contain the newer builds above):

- [macOS arm64 archive](../artifacts/Git-View_0.1.0_macos-arm64.zip), 3,319,623 bytes.
  Runnable app: `src-tauri/target/release/bundle/macos/Git View.app`.
- [Windows x64 installer](../artifacts/Git-View_0.1.0_windows-x64-setup.exe), 2,609,886 bytes.
  Original: `C:\codeme\git-view\src-tauri\target\release\bundle\nsis\Git View_0.1.0_x64-setup.exe`.
- [SHA-256/source manifest](../artifacts/manifest.json),
  [Mac native report](../artifacts/native-macos.json), and
  [Windows native report](../artifacts/native-windows.json).

Existing README/runbook/setup-document changes remain preserved. Application
source is checkpointed locally; final documentation and verification-tooling
refinements remain uncommitted. No branch was pushed and no release published.

Limits: release artifacts are unsigned/not notarized. Linux CI and Intel Mac
execution have not run. Remote-service authentication, complete screen-reader/
Tab traversal, and long-group scroll restoration were not verified. APFS rejects
the raw-invalid-UTF-8 filename fixture; tab/newline paths and byte parsing remain
tested. Crash tests inject interruptions and restart engines rather than cutting
power. Combine retains its candidate worktrees and backup/replacement refs;
sparse and unsupported signing/config contexts are rejected explicitly.
No known blocker remains for the implemented Mac/Windows milestone workflows.

The sections below retain the checkpoint history, including failures fixed
before the final successful gates.

## 2026-09-19: prerequisite gate

Source baseline: `e69eedae94ff6d4f961dbdf048b4133f60f53e82` on Mac and Windows.
Existing user changes to `README.md`, `docs/OVERNIGHT.md`, and untracked
`Dev-Machine-Setup.md` are preserved. Nothing pushed or published.

Coordinator owns scaffolding, contracts, orchestration, Windows synchronization,
builds, and this record. Workers start after the native scaffold/contracts gate,
with exclusive ownership as specified in `OVERNIGHT.md`.

Prerequisite evidence:

- User clarified that `runsup` meant `rustup`.
- Mac: rustup 1.29.1, Rust/Cargo 1.98.1, rustfmt and Clippy installed and on PATH.
- Mac: Node 22.22.0, pnpm 9.12.0, Git 2.51.2; Apple Command Line Tools available.
- Mac: standalone `rustc` compilation and executable probe passed.
- Windows: SSH BatchMode login succeeded; `C:\codeme\git-view` is a clean checkout.
- Windows: Rust/Cargo 1.98.1, rustfmt and Clippy installed; MSVC target selected.
- Windows: WebView2 runtime directory version 153.0.4234.32 exists; GUI smoke pending.
- Windows: Node 22.22.2 found in fnm storage but absent from SSH PATH; pnpm absent.
- Windows: standalone Rust compilation failed: `link.exe` not found.
  Visual Studio Professional 2022 is installed without detected C++ compiler tools.

Reproducible remote checks: `python3 scripts/windows-remote.py scripts/windows-preflight.ps1`.
Setup script prepared to expose existing Node for the build session, install pnpm
in the checkout-local `.tools` directory, and add the Visual Studio C++ workload.

## Scaffold and M1 integration

Windows prerequisites resolved: pnpm 9.12.0 installed under checkout `.tools`,
existing Node 22.22.2 exposed in the build process, Visual Studio C++ workload
installed. Native Rust probe now passes. No global agent or sandbox settings changed.

Local checkpoint `aea3041e0f7612ebcd9830167aaabfcc7e4dd178` contains the scaffold,
frozen contracts/fixtures, and initial worker slices. Transferred with
`python3 scripts/windows-sync.py`; Windows dedicated branch `git-view-build`.
Windows `python3 scripts/windows-remote.py scripts/windows-build.ps1` passed:
locked dependency installation, frontend production build, native debug build.
Mac `pnpm build` and `cargo build --manifest-path src-tauri/Cargo.toml` passed;
minimal native process launched. A process launch alone is not the workflow smoke.

Workers, ownership, and latest evidence:

- Git worker: `src-tauri/src/git/**`, `tests/git_engine.rs`; B1–B4 implemented.
  23 real-repository integration tests pass, six runner tests pass. One ignored
  performance measurement was run explicitly: 10,000-commit first page 44.00 ms;
  25 sequential status reads 173.46 ms, macOS arm64 debug (before final integration).
- Repository worker: `src/features/repository/**`, `src/graph/**`; R1–R3 implemented.
- Group worker: `src/features/groups/**`; G1–G3 implemented.
- `pnpm typecheck`, `pnpm lint`, `pnpm test`: pass (24 UI/graph tests at checkpoint).
- Coordinator: project catalog, bounded operations, launch queue, native registration,
  shell, native smoke harness and release/platform verification.

Integrated backend acceptance command:
`cargo test --manifest-path src-tauri/Cargo.toml --test acceptance -- --nocapture`
passed 5 tests: mixed group success/dirty skip/divergence/missing/alias/filter/retry;
stale status and project refusal; strict schema and revision-preserving saves;
idempotent action plans; 100-member cancellation/scheduling.

Cross-review fixes include unborn staged-then-edited unstage, branch mergeOptions
overriding pull policy, GUI unchanged-status parsing, stale preview/Save-as races,
project save serialization, Unix backslash preservation, cancellation while waiting
for shared locks, and mutation timeout uncertainty.

Contract additions: `AppError.exitCode?` and `outputTruncated?` retain failed Git
diagnostics; typed project/batch/query shapes in `src/api/types.ts`. Native smoke
instrumentation is gated behind Cargo `native-smoke` and `VITE_NATIVE_SMOKE=1`,
and excluded from normal release builds.

## M1 local acceptance gate passed

Mac native smoke passed at `06a67843d797dba0bf5533c828dd3ba224a65635`:
`VITE_NATIVE_SMOKE=1 pnpm build`,
`cargo build --manifest-path src-tauri/Cargo.toml --features native-smoke,tauri/custom-protocol`,
`python3 scripts/native-smoke.py /tmp/git-view-native-m1-mac-warm.json`.
All 11 real DOM/IPC checks passed: mixed group pull, historical/working/merge-parent
diffs, one-file stage/commit, explicit local push, failed hook/draft retention,
invalid launch rejection, warm welcome, invoking-cwd repository and project routing.
Dirty fixture file content remained unchanged. Tests use disposable local remotes.

`pnpm tauri build --bundles app` produced the normal unsigned Mac arm64 release at
`src-tauri/target/release/bundle/macos/Git View.app` (8.3 MiB at this gate).
Smoke instrumentation was excluded. `python3 scripts/release-smoke.py
'src-tauri/target/release/bundle/macos/Git View.app/Contents/MacOS/git-view'`
passed help/version/nonzero-invalid-path and 3-second packaged-process startup.
This startup check is separate from the full WebView workflow checks above.

M1 local core accepted before beginning M2. Windows platform acceptance continues:
at `af24285fe7e2d2141338eec78ca990487513f267`, frontend 24/24, Rust unit 10/10,
integrated acceptance 5/6 passed, including mixed pull and 100-member cancellation.
One Windows-specific relative path assertion failed; correction uses canonical
paths without Windows extended-prefix mismatch. The earlier test executable
startup failure was fixed by embedding Common Controls v6 in test executables.
See [Microsoft manifest guidance](https://learn.microsoft.com/en-us/windows/win32/controls/cookbook-overview).

M2 ordinary merge and squash-merge implementation now assigned to original Git
and repository owners. M3 remains behind M2 acceptance and recovery-design gates.

Windows M1 gate at `d5895bd88cb446a7332d560489ace4808f4e5c0f`: typecheck/lint,
24 frontend tests, 10 Rust unit tests, 6 integrated acceptance tests, 22 Git tests
(Unix-only byte-path test not applicable), and strict Clippy passed. One explicit
performance benchmark remains ignored in default suites. Native WebView smoke
timed out under SSH session 0; rerunning through a temporary Interactive scheduled
task in Brian's active desktop session passed all 11 real workflow/CLI checks.
Report: `C:\codeme\git-view\.tools\GitViewNativeSmoke-59807fe367a1446bb3c5488636452012.json`.
`scripts/windows-desktop-smoke.ps1` removes its temporary task after execution.

M2 initial Mac native smoke passed 16 combined M1/M2 checks. Cross-review then
hardened merge metadata preconditions, squash-journal cleanup races, and external
commit/branch reconciliation. Backend evidence: 16 M2, 23 M1, 12 unit tests passed;
strict Clippy passed. Native rerun pending to close the M2 gate.
M2 native rerun after review fixes passed all 16 checks; report
`/tmp/git-view-native-m2-mac.json`. M2 local gate is accepted before starting M3.
M3 design recorded in `docs/REWRITE_RECOVERY.md`; implementation is now authorized
under the existing Git/repository worker ownership.

Windows M2 gate passed at `f2e5021f0cc9bbb937ad8c291fb3fd6286b26283`:
28 UI/graph tests, 11 platform-applicable unit tests, 6 integrated acceptance tests,
22 M1 Git tests, 16 merge tests, strict Clippy, and all 16 native desktop checks.
Report: `C:\codeme\git-view\.tools\GitViewNativeSmoke-b177cb750269477ab3e3e0b785073b15.json`.
Git for Windows is 2.53.0.windows.1. Explicit Windows debug benchmark
`cargo test --manifest-path src-tauri/Cargo.toml --test git_engine acceptance_performance_measurements -- --ignored --nocapture`
passed: 10,000-commit first 200 rows 190.90 ms; 25 sequential status reads 1405.05 ms.
This excludes GUI rendering; native virtualization/timing is part of final smoke.

M3 engine, UI and native scenarios are implemented against the shared action/recovery
contracts. Independent review corrected missing-ref reconciliation, symbolic-ref
transaction races, hook-added replacement refs, journal overwrite after tampering,
and post-hook recovery revalidation. Twenty M3 integration cases pass. The unit
suite also exercises twelve install and three recovery interruption points,
backup collision refusal, and bounded guard subprocess cleanup. Recovery preserves
later staged, unstaged and untracked bytes; later commits require manual review.
Candidate worktrees and backup/replacement refs are retained for durable recovery.
Unsupported signing/config context differences are explicitly rejected.

Final native acceptance is in progress. The first extended Mac run timed out
without a final report; it is not counted as a pass. The current harness adds
per-check progress files and an actual WebView reload during a running operation
to diagnose and verify the complete sequence. No M3 native acceptance claim yet.

M3 checkpoint `595cbcc0a722c62604046fe19db7b54876617930`: Mac full Rust suite
82 passed (one performance test explicitly ignored), frontend 34 passed,
typecheck/lint/format and strict native-smoke Clippy passed. Native testing found
the failed-combine journal panel did not refresh when Git status was unchanged;
the UI now refreshes records on each explicit/completed-operation refresh, with
a regression test. A subsequent intermittent focus assertion now waits for the
connected focused editor across rendered frames and reports focus diagnostics.

Windows at that checkpoint passed 34 frontend tests, then exposed a rewrite
test stack overflow. Reproduced locally with a 1 MiB stack: moved subprocess IO
buffers and the shared subprocess future onto the heap. The same constrained
stack regression now passes; no stack limit was increased. Full platform reruns
and final packaging remain required before the final acceptance claim.
Linux CI workflow added but not executed; no Linux verification claim.
User's original README/runbook/setup-doc edits remain preserved and uncommitted.

## M3 local acceptance passed; final platform packaging

Source revision: `f40c7248bd3b5f8b4d2de1e11b794bb81f2d5430`.
Mac full locked Rust suite: 83 passed (17 unit, 7 coordinator acceptance,
23 M1 Git, 16 M2 merge, 20 M3 rewrite), one performance test explicitly ignored.
Frontend: 34 passed; typecheck/lint/format and strict Clippy pass.
The 1 MiB constrained-stack regression reproduces the former Windows crash
without the fix and passes with heap-backed subprocess state/buffers.

`VITE_NATIVE_SMOKE=1 pnpm build`;
`cargo build --manifest-path src-tauri/Cargo.toml --features native-smoke,tauri/custom-protocol`;
`python3 scripts/native-smoke.py /tmp/git-view-native-m3-mac.json`: **26/26 passed**.
This includes all M1/M2 workflows; M3 exact two-tip replacement, message/parent/tree
and backup verification, recovery preserving later staged/unstaged/untracked
content, rejecting hooks, and refusal after later commits. Additional native
checks cover keyboard history navigation/graph alignment, actual light/dark
window themes and body contrast, two window sizes, visible editor focus,
group selection/filter retention, and a full WebView reload during a running
commit followed by operation/navigation/status recovery.

Mac arm64 debug native measurements: warm CLI subprocess through welcome render
172.0 ms; creation of the 10,000-commit fixture plus navigation/first 200-row layout
543.0 ms; scroll including two animation frames 20.0 ms. Virtualized DOM rows:
24 before/22 after scrolling, with changed visible OID. Fixture creation is
included in the 543.0 ms figure; backend-only measurements are recorded above.

Windows full rerun and final normal release/installer checks are in progress.

Windows `f40c724` passed all 15 applicable unit tests (including the constrained
stack case), 6 coordinator acceptance tests, 22 M1 Git tests, and 16 M2 tests.
One of 20 M3 tests exposed extended Windows paths being passed to an external
SSH signing program. Revision `6a28ef976bd92f8baf844bdc04f91a19764dd3b9`
normalizes only helper configuration paths; canonical repository/journal identity
is preserved. Its focused Mac signing regression and strict Clippy passed.
Both platforms are now checking this final source revision.

Final Mac native rerun at `6a28ef9`: all 26 checks passed; retained report
`artifacts/native-macos.json` (original `/tmp/git-view-native-final-mac.json`).
Warm welcome 199.0 ms; fixture creation plus 200-row render 347.0 ms; scroll
35.0 ms; virtual DOM 24/22 rows. These are individual debug measurements,
not a statistical benchmark. Normal `pnpm tauri build --bundles app` passed
with smoke instrumentation excluded, producing the 8.7 MiB arm64 `.app`.

Final Windows gate at `6a28ef9` passed: 79 backend tests (15 unit,
6 coordinator acceptance, 22 M1 Git, 16 M2 merge, 20 M3 rewrite), 34 UI tests,
typecheck/lint, strict Clippy, and all **26 native desktop checks**.
Command: `python3 scripts/windows-remote.py scripts/windows-verify.ps1`.
Report: `C:\codeme\git-view\.tools\GitViewNativeSmoke-16fc72e913a647b385a771d081ecfe38.json`.
Windows debug native timings: warm welcome 166.8 ms; creation of the 10,000-commit
fixture plus first 200-row layout 4099.2 ms; scroll/two animation frames 3.8 ms;
virtual DOM 25/24 rows. Release compilation ran concurrently, so these are
observations under build load, not baseline performance guarantees.

Final Mac full locked backend rerun at `6a28ef9` also passed all 83 tests and
format checks. The final packaged Mac release again passed help/version,
nonzero invalid-path handling, and native process startup via
`python3 scripts/release-smoke.py 'src-tauri/target/release/bundle/macos/Git View.app/Contents/MacOS/git-view'`.

## Final Windows package and artifact collection

`python3 scripts/windows-remote.py scripts/windows-release.ps1` passed at
`6a28ef9`: optimized x64 executable, normal frontend without smoke assets,
NSIS installer, and release CLI help/version. NSIS tools were downloaded and
hash-validated by Tauri. Installer size: 2,609,886 bytes.

`python3 scripts/windows-remote.py scripts/windows-installer-smoke.ps1` passed:
silent install to a fresh checkout-local test directory, installed CLI help/
version, and an actual visible native window titled `Git View` in the interactive
desktop session. Test install retained at
`C:\codeme\git-view\.tools\installed-smoke-af56620c63614eccb1147813ebe6cbd8`.
The temporary scheduled task and application process were stopped/removed.
The first attempt hit Windows' encoded command-line length limit before running
the installer. The remote helper now transfers scripts through UTF-8 stdin with
a short fixed bootstrap; the rerun passed. Installer checking also verifies the
window title, so a console window cannot satisfy that assertion.

`python3 scripts/collect-artifacts.py C:/codeme/git-view/.tools/GitViewNativeSmoke-16fc72e913a647b385a771d081ecfe38.json`
collected both releases, checked matching source revisions, copied the passing
native reports, and wrote `artifacts/manifest.json` with sizes and SHA-256 hashes:

| Artifact | SHA-256 |
| --- | --- |
| macOS arm64 ZIP | `24cd1d83760ef4d5b08282bdde02b79800b9d518ced087942fcea99e48407092` |
| Windows x64 NSIS | `b4e091050f4dd0be5b5b853fac5ab80c87733e455e1e06abaa2dae8b04d33415` |

For development: `pnpm install --frozen-lockfile`, then `pnpm tauri dev`.
For a project group, copy `docs/examples/work.git-view.json` and replace its
repository paths; relative paths resolve beside that JSON file. Open it from
the application or with `scripts/git-view group /path/to/work.git-view.json`
(`scripts\git-view.cmd` on Windows). Both launcher scripts preserve invoking cwd.
