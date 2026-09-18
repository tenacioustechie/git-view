# Git View

A planned lightweight desktop Git client for macOS, Windows, and Linux. Launch directly into one repository or a saved group of repositories from the command line.

**Status: planning only.** No application has been implemented yet. This plan was prepared on 18 September 2026 for an overnight build. Tauri/Rust/TypeScript and project-file groups are confirmed; remaining assumptions are recorded in the decisions.

## Build documents

| Document | Purpose |
| --- | --- |
| [Product and UX](docs/PRODUCT.md) | Scope, screens, CLI behavior, workflows, and feature milestones |
| [Decisions](docs/DECISIONS.md) | Confirmed requirements, open questions, and working defaults |
| [Architecture](docs/ARCHITECTURE.md) | Stack, module boundaries, history rendering, persistence, and platforms |
| [Git behavior](docs/GIT_ENGINE.md) | Exact operation semantics, parsing, concurrency, and failure handling |
| [API contracts](docs/CONTRACTS.md) | Shared data and command contracts for independent implementation |
| [Project files](docs/PROJECT_FILES.md) | Portable JSON group format, schema, and examples |
| [Overnight build](docs/OVERNIGHT.md) | Dependencies, subagent ownership, checkpoints, and kickoff prompt |
| [Acceptance](docs/ACCEPTANCE.md) | Meaningful tests, platform checks, and morning handoff criteria |

Agreed stack: Tauri 2, Rust, and a TypeScript UI; React is the proposed UI framework. Use the user's installed Git. Tauri uses the operating system's webview, which supports the small distribution goal. Actual memory use and launch speed must be measured. See [Tauri's introduction](https://tauri.app/start/).

Planned CLI examples, **not commands available today**:

```sh
git-view .
git-view repo ~/code/api
git-view group ./work.git-view.json
git-view group --scan ~/code --depth 3
```

The overnight target is a useful local build with history, commit inspection, whole-file staging, commit, fetch/pull/push, and a group dashboard with batch fetch and branch-filtered pull. Both squash-merging and combining existing commits are confirmed requirements. Merge and squash-merge follow the core integration gate; combining existing commits has a separate milestone with backup and recovery support.

Start with [the decisions](docs/DECISIONS.md), then use [the execution plan](docs/OVERNIGHT.md). This documentation does not schedule or launch an overnight coding session.
