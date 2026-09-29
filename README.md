# Git View

A lightweight desktop Git client built with Tauri, Rust, TypeScript, and React. Launch directly into one repository or a saved JSON group from the command line.

**Using the app:** start with the [User guide](docs/USER_GUIDE.md).
The current source opens repositories in **Changes** by default, with a file list, dedicated
diff preview, and commit composer. Check individual files for **Stage selected**
or **Unstage selected**; clicking a filename only previews it. Choose **History** for the graph and branch
controls. **Workspace** holds opening/discovery actions; **Recent** reopens the
last 20 repositories/projects and remembers them across app restarts. The current-branch menu
opens branch workflows; **More (…)** holds Combine commits. History includes a
filterable commit-file browser and Show/Hide details; Changes and History diffs
have line-number gutters and full-row change colours. **View** saves your opening
workspace, commit-detail visibility and panel widths; drag or keyboard-adjust the
panel dividers, or use its sliders and reset control. In compact windows, History's
branch/scope navigation opens from a button above search, leaving more room for
the commit inspector. These design passes
include an app-bar **Shortcuts** reference, menu key hints, and keyboard focus
improvements for project filters, history and panels. A read-only repository
**Activity** drawer adds attention filtering, operation times and expandable
diagnostics. **Diff layout** switches working/history/stash text previews between
Unified and Side-by-side; working/history previews also offer hunk navigation.
**File history** opens paged, read-only path history from a selected working or
historical file, with explicit revision scope and current/previous rename paths.
These changes
are not yet in the collected native packages.
See [Feature status](docs/FEATURE_STATUS.md) for what is implemented and what remains.
Future user-facing changes must update both documents in the same change; see
[Repository instructions](AGENTS.md).

**Implementation and verification:** see the [Build and installation guide](docs/BUILD_GUIDE.md)
for local builds, package installation, and the current GitHub Actions pipeline.
See [Build status](docs/BUILD_STATUS.md) for tested source revisions, native
evidence, artifact paths, and remaining platform checks. Local macOS and Windows
workflows are exercised with disposable repositories; Linux CI is configured
but has not run.

Install the pinned dependencies with `pnpm install --frozen-lockfile`. Run `pnpm tauri dev` for development. On macOS, `scripts/macos-build.sh --skip-deps --open` builds and opens the current source; add `--check --install` to run the full checks and replace `/Applications/Git View.app`. On Windows, use `.\scripts\windows-build.ps1 -SkipDeps -Launch`; add `-Check -Install` for full checks and per-user installation. Use `scripts/git-view` on macOS/Linux or `scripts\git-view.cmd` on Windows after building. These launchers preserve the invoking working directory. They are not installed globally.

## Build documents

| Document | Purpose |
| --- | --- |
| [User guide](docs/USER_GUIDE.md) | Opening repositories, groups, discovery, history, commits, text-hunk staging, remote actions, cherry-pick, guided rebase, stashes, merges, and recovery |
| [Build and installation guide](docs/BUILD_GUIDE.md) | Prerequisites, local development, source checks, release packages, installation, and the current GitHub Actions pipeline |
| [Feature status](docs/FEATURE_STATUS.md) | Current completion inventory, remaining roadmap, and verification gaps |
| [Next feature stage](docs/NEXT_STAGE.md) | Confirmed delivery sequence, feature boundaries, acceptance criteria, and implementation preparation |
| [Development machine setup](Dev-Machine-Setup.md) | Mac and Windows prerequisites, SSH access, and readiness checks |
| [Product and UX](docs/PRODUCT.md) | Scope, screens, CLI behavior, workflows, and feature milestones |
| [Decisions](docs/DECISIONS.md) | Confirmed requirements, open questions, and working defaults |
| [Architecture](docs/ARCHITECTURE.md) | Stack, module boundaries, history rendering, persistence, and platforms |
| [Git behavior](docs/GIT_ENGINE.md) | Exact operation semantics, parsing, concurrency, and failure handling |
| [API contracts](docs/CONTRACTS.md) | Shared data and command contracts for independent implementation |
| [Project files](docs/PROJECT_FILES.md) | Portable JSON group format, schema, and examples |
| [Overnight build](docs/OVERNIGHT.md) | Dependencies, subagent ownership, checkpoints, and kickoff prompt |
| [Acceptance](docs/ACCEPTANCE.md) | Meaningful tests, platform checks, and morning handoff criteria |

Git View uses the installed Git and operating system webview. Normal hooks, signing, and credential helpers remain in effect. Pull is explicitly fast-forward-only and skips dirty repositories; it never automatically stashes or rebases.

Local launcher examples:

```sh
scripts/git-view .
scripts/git-view repo ~/code/api
scripts/git-view group ./work.git-view.json
scripts/git-view group --scan ~/code --depth 3
```

The application includes paged history, scoped message/author/commit-ID search, parent-specific commit inspection, whole-file staging, commits, fetch/pull/push, local branch creation/switching, reviewed remote tracking checkout, single-commit cherry-pick with persistent conflict guidance, and a project dashboard with batch fetch/pull and per-member results. Project files can be created and edited; external changes cannot be silently overwritten. Ordinary merge and squash-merge have separate workflows. Combine commits uses backup/recovery safeguards described in [the recovery design](docs/REWRITE_RECOVERY.md); consult build status for its current acceptance evidence.

In a repository, choose a branch or **All branches and tags**, then use **Search history**. Message search includes the full commit body; author search matches names and emails; commit-ID search accepts at least four hexadecimal characters. Message and author queries are literal text. Search results omit graph connectors across excluded commits. **Clear search** restores the graph; `⌘F` / `Ctrl+F` focuses search.

Project files use version 1 of the [documented JSON format](docs/PROJECT_FILES.md); paths resolve relative to the project file. Discover repositories scans a folder to a chosen depth, then lets you select repositories before creating a project or adding to its draft. Scanning never saves membership automatically. Stash creation, inspection, retained Apply, reviewed Delete, complete text-hunk staging, and guided linear rebase are implemented and tested on macOS/Windows; see Build status. Individual-line staging, merge-preserving or arbitrary interactive rebase, and force push are not implemented. Releases are unsigned local builds; nothing is pushed or published by the build scripts.
