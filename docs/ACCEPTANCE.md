# Acceptance and verification plan

These are build acceptance criteria. Current results and platform evidence are recorded in [Build status](BUILD_STATUS.md). Documentation/schema validation is separate from app validation; code presence alone does not establish acceptance.

## First integration gate: the user's group workflow

Create temporary local repositories and a project JSON file with paths relative to that file. Include a clean repository behind its remote, a dirty repository, a diverged repository, and a missing path. Open the project from a CLI whose cwd is elsewhere.

- All rows appear with the right branch, working state, and availability.
- Pull all previews the entire candidate set; dirty/missing entries show skipped reasons.
- The clean behind repository advances to the upstream commit.
- The dirty repository's HEAD, index, tracked/untracked content, and branch remain unchanged by the batch.
- Divergence reports failure and preserves local commits; no automatic merge/rebase/stash occurs.
- One failure does not suppress another repository's success. Results remain visible.
- An exact branch filter excludes other checked-out branches without switching them.
- Clearing that filter allows each eligible repository to pull its own configured upstream.
- Open a skipped repository, resolve it manually, refresh, and retry only the intended entries.

## Backend tests with real Git

Generate isolated repositories under test-owned temporary directories and local bare remotes. Isolate test Git config, identities, hooks, signing, credential helpers, and file-protocol needs so tests never depend on or change user config. Hooks/signing failures are enabled only in their dedicated fixtures. Clean up only test-owned paths.

| Fixture | Assertion |
| --- | --- |
| Unborn, detached, missing/deleted upstream | Correct state; useful action availability; unknown counts stay unknown |
| One file with staged and unstaged edits | Both states visible; commit contains staged content only |
| Initial-repository unstage | Index cleared for selection; working content remains |
| Rename, deletion, Unicode/spaces, leading dash, literal pathspec characters | Actions round-trip exact selected paths; no accidental glob selection |
| Tabs/newlines/non-UTF-8 paths where supported | Byte-safe parsing; no lossy path actions; platform-specific cases clearly gated |
| Root commit, two-parent and octopus merges | Actual parent relationships preserved; root/selected-parent diffs correct |
| Shallow clone, missing object, binary/oversized file | Explicit boundary/unavailable/truncated state, no false history or hang |
| Ref changes between history pages | No pages mixed across snapshots; refresh/restart is explicit |
| User pull config enabling rebase/autostash | Batch still obeys fast-forward-only and dirty-skip policy |
| Branch/upstream/HEAD changes after preview | Stale plan rejected or skipped before mutation |
| Shared worktrees and duplicate aliases | Separate worktree state; common lock serializes mutations; aliases not double-pulled |
| Dirty submodule | Dirty status and skip policy; no recursive updates |
| Commit hook failure or hook modifying index | Draft retained; actual resulting state refreshed; no automatic rollback |
| Existing merge/rebase/cherry-pick | Correct persistent state; incompatible actions blocked |
| Push target config, follow-tags, recursive push settings | Only displayed destination/ref published; unintended tags/submodules untouched |
| Lockfile, unavailable Git, unreachable remote | Useful failure, bounded job lifetime, no lock deletion |
| Cancel queued job / interrupted running job | No false success; refresh and accurate uncertain outcome where needed |
| Excessive structured command output | Explicit limit error or complete streamed parse; never partial “clean” result |

M2 adds ordinary merge success/conflict/continue/abort and squash-merge staged review, including a conflict with no `MERGE_HEAD`. M3 rewrite tests must exercise backup creation, interrupted phases, hook/signing failure, and recovery before enabling history squash.

Both squash workflows are required for full product acceptance. For Combine commits, verify that at least two selected consecutive tip commits become one commit with the chosen message, the final tree remains unchanged, the replacement has the correct parent, and the backup retains the original tip. Reject unsupported selections without mutation. Verify recovery restores the original branch history without discarding later user changes; stale recovery preconditions must require manual review.

## Project and CLI tests

- Valid schema/example, empty project, duplicate names, missing members, unknown keys, unsupported version, corrupt JSON, nonblank strings, and file-size/member-count limits.
- Relative paths resolve from the project file; direct repository arguments resolve from invoking cwd. Test absolute paths, Windows drives, spaces, and Unicode on real target systems.
- Duplicate aliases share execution identity; separate worktrees remain separate.
- Save/reload retains order and labels. Read-only file can open; external edits cannot be overwritten by a stale save.
- Corrupt external project edits show stale/error state and prevent starting a new batch from hidden membership assumptions.
- Cold launch, second launch while open, nested repository directory, invalid explicit path, and no-argument welcome behavior.
- `--help` and `--version` do not start the GUI. Initial launch is not lost before event listeners attach.

## UI tests and native smoke

Use unit tests for graph lane/parent invariants and reducers; component tests for selection, action states, and retained drafts. Assert observable behavior rather than mirroring component internals. Mock IPC only where the test explicitly concerns renderer behavior.

Native smoke sequence: launch from CLI; open project; run mixed-result batch pull; enter repository; inspect a merge commit against each parent; inspect a working file; stage only one of two changes; commit; inspect resulting history; push to a local remote; switch back to group and observe refreshed status. Repeat commit with a failing test hook and confirm the draft survives.

Check dark/light mode, readable focus states, keyboard navigation, no color-only status, long branch/path truncation with full details available, window resizing, and history row/edge alignment. Reload the UI during an operation and verify querying operation state repairs missed events.

Measure warm shell startup, first 200 history rows on a 10,000-commit fixture, 25-repository status sweep, and scrolling responsiveness. Record machine, build mode, dataset, and numbers. Exercise 100 repositories for bounded scheduling and partial results without declaring that all 100 must finish in the 25-repository budget. Do not load every repository's entire history just to show its status.

## Platform matrix

| Target | Required for a verified platform claim | Expected overnight evidence |
| --- | --- | --- |
| Local macOS architecture | Backend/UI checks, native CLI smoke, runnable `.app` | Actual local evidence if setup succeeds |
| Other macOS architecture | Native/cross-build plus native execution evidence appropriate to claim | Pending unless available |
| Windows 11 x64 | Native build, backend tests, paths/process/credentials smoke, installer launch | CI config initially; actual run only if a runner exists |
| Ubuntu 24.04 x64 | Native build with system dependencies, backend tests, GUI and package smoke | CI config initially; actual run only if a runner exists |

A committed workflow is not a passed job. A compiled binary is not an installer smoke test. Report each separately. Credentials and hooks must be exercised on disposable repos; remote-service authentication smoke needs an explicitly selected test destination.

## Commands to establish in the scaffold

The scaffold provides `pnpm typecheck`, `pnpm lint`, `pnpm test`, `pnpm build`, `pnpm tauri dev`, and `pnpm tauri build`. Rust checks use `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, and `cargo test --manifest-path src-tauri/Cargo.toml --locked`. Native and platform scripts are under `scripts/`; exact executed commands are recorded in Build status.

Run the checks appropriate to each slice, then the full required gate once integrated. Repeat only for new changes, failures, or unresolved risks. Do not claim all checks passed when a command was unavailable, skipped, or only run against mocks.

## Morning handoff

- A working artifact path or exact verified development launch command.
- The project file example and directions for pointing it at real checkouts.
- Completed M1/M2 items, explicitly unfinished features, and any fallback such as hand-editing project JSON.
- Exact check commands and outcomes, with mock UI/native/backend distinctions.
- Tested OS/architecture and pending platforms; signing/installer status.
- Measured performance where available, known Git/auth/path limitations, and the next task.

M1 is complete only when the real group and repository workflows pass, CLI launch works, and the local native build is usable. Overall cross-platform delivery remains open until the corresponding native checks pass.
