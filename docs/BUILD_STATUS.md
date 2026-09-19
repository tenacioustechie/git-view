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

M1: integrated, native smoke and Windows integrated acceptance still pending.
M2 and M3: intentionally gated until M1 accepted. No release artifacts yet.
Linux CI workflow added but not executed; no Linux verification claim.
User's original README/runbook/setup-doc edits remain preserved and uncommitted.
