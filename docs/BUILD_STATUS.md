# Build status

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
Linux CI workflow added but not executed; no Linux verification claim.
User's original README/runbook/setup-doc edits remain preserved and uncommitted.
