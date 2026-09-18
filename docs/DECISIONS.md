# Decisions and open questions

Prepared: 2026-09-18, Australia/Melbourne. Proposed overnight window: evening of 18 September into 19 September. The start time, duration, and implementation kickoff are not yet set.

## Confirmed by the user

- Support macOS, Windows, and Linux.
- Launch from a CLI into one repository or a repository group.
- Display a Git history DAG, commit details, and changes.
- Provide Git actions including commit, pull, push, fetch, merge, and squash.
- Squash includes both squash-merging another branch and combining existing commits (confirmed 19 September 2026). Both are required product features.
- Show group repository states, including clean/dirty and current branch.
- Support operations across repositories, including pulling repositories on a branch.
- Ask product questions and prepare a plan and build docs for overnight work with subagents.
- Use Tauri, Rust, and a TypeScript UI (confirmed in follow-up).
- Define repository groups in a project file; JSON is the selected proposed format.
- Prioritize group status and quickly pulling the latest changes across repositories.
- Skip dirty repositories and let the user handle their changes or conflicts manually.

## Questions asked; answers pending

| Question | Proposed default if unanswered | Effect of a different answer |
| --- | --- | --- |
| What should tonight deliver: working core, broad coverage, or polished prototype? | A working core with real Git operations | Reorder milestones; do not label untested operations complete |
| Does pull on a branch include switching branches? | Only repositories already on that exact local branch | Add a separate previewed batch switch action; do not hide it inside pull |
| How many repositories and what additional workflows matter? | Typical 5–30, exercise 100; existing worktrees supported; external conflict editing | Larger groups, submodule management, or a conflict editor materially increase scope |

These are planning assumptions. Update this table and dependent docs when the user answers. Absence of an answer is not permission to install software, start a scheduled run, or publish a release.

Resolved follow-up: the user accepted Tauri/Rust/TypeScript and asked for project-file groups. The plan uses explicit JSON membership with relative paths. React, file format details, fast-forward policy, and release scope remain implementation recommendations. Prioritize the working group workflow when allocating overnight time.

## Other working decisions

| Topic | Working decision | Reason |
| --- | --- | --- |
| Product name / CLI | Git View / `git-view` | Matches the workspace; branding can follow |
| Git integration | Installed Git subprocesses, typed backend commands | Reuses Git configuration, credentials, hooks, and repository compatibility |
| Windows / Linux targets | Windows 11 x64; Ubuntu 24.04 x64 initially | Concrete first test targets; widen after real validation |
| macOS targets | Current development Mac first; Apple silicon and Intel release targets | Do not equate one successful local build with both architectures tested |
| Remote authentication | Existing Git credential helpers and SSH agent | No hosting account system needed for first version |
| Groups | JSON project files with explicit members and paths relative to the file | User-requested setup; portable across similar checkouts |
| Batch actions | Fetch and fast-forward pull first; push and switch later | Keep the essential multi-repository workflow achievable overnight |
| Refresh | On focus, after actions, manual, bounded visible dashboard polling | Useful external-change detection without a complex watcher in the first build |
| Conflicts | Show files; resolve externally; stage/continue/abort where supported | A full three-way editor is a separate milestone |
| Default UI | React; compact three-pane desktop layout; system light/dark theme | Prioritizes readable history and dense group status |
| Installation | Local unsigned Mac build first; native CI for other OSes | Signing credentials and remote CI access are not established |
| Offline use | History, status, diffs, and local actions work offline | Network operations fail independently and remain retryable |

## Environment observed during planning

The repository has no commits and no application files. There were no applicable ancestor `AGENTS.md` files found during inspection. Git 2.51.2, Node 22.22.0, npm 10.9.4, pnpm 9.12.0, and Xcode Command Line Tools are available. `rustc` and `cargo` were not on PATH; the usual `~/.cargo/bin` and `~/.rustup` directories were absent.

Rust setup and dependency downloads are the first implementation prerequisites. The current session has restricted network access and protected paths, so some setup/build actions can require runtime permission. Resolve actual setup prompts while the user is available; do not assume an overnight run can approve them itself.
