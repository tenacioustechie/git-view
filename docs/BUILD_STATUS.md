# Build status

## History search: Mac integration passed; Windows verification pending

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

Review fixes: cursors retain concurrent issued continuations; search and normal
history resolve HEAD without status cleanup; historical metadata/file lists/diffs
all ignore replacement refs consistently; shallow/graft changes expire a page.
Queries are limited to 512 UTF-8 bytes and 30 seconds; overly broad OID prefixes
request refinement. Unsubmitted Clear and dismissed-error states no longer leave
loading stuck or falsely claim zero matches. Two new fixture setup mistakes were
corrected before the full successful backend rerun.

The release artifacts and completed checks below still describe the discovery
checkpoint until the search integration gate records their replacement.

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
