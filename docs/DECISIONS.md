# Decisions and open questions

Initial planning record: 2026-09-18, Australia/Melbourne. The overnight timing and
environment notes below are historical, not the current implementation status.
See [Feature status](FEATURE_STATUS.md) and [Build status](BUILD_STATUS.md) for
current completion and verification.

## Next-stage decisions — 21–22 September 2026

Implementation continuation, 4–5 October: following the request to proceed with
group operations, B1 starts with one shared existing local branch name, consistent
with the shared-branch usage below. Each repository's source/target commits are
reviewed separately. Per-member target editing and remote branch creation are
excluded from this initial slice; there is no implicit stash/pull or rollback of
successful members. B2 progressive status remains next; B3 batch push still needs
its destination-policy choice. This is the implementation scope, not a claim that
the user separately selected every engineering detail.

- Confirmed order, expanded after the workflow discussion: remote tracking
  checkout → cherry-pick → stash → hunk staging.
- Confirmed delivery: small feature slices, each tested and usable before the next.
- Confirmed usage: up to 15 repositories per group, shared branch names, and
  `main` as the usual default branch. This does not authorize implicit branch switching.
- Everyday tasks include push, pull, merge, and cherry-pick; the user uses both
  terminal and visual tools for simple operations. Clarification: complex merges,
  rebase, and unclear state drive the need for a visual tool. No specific failure
  of the existing push/pull/merge implementation was reported.
- Confirmed stash scope: Apply keeps the stash; Delete is a separate reviewed
  action. Include untracked files is initially unchecked; ignored files stay excluded.
- Confirmed 22 September: rebase belongs to the following stage, after the four
  slices above.
- Confirmed 22 September: first improve complex conflicts with branch/commit
  context, a conflict list, and next-step guidance; resolve files externally.
  An integrated three-way editor is deferred.
- The [implementation plan](NEXT_STAGE.md) records the confirmed product scope
  and engineering assumptions. A1–A4 are delivered on the tested macOS/Windows
  platforms. Rebase follows in a separately designed stage.

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

The original group-size question was answered on 21 September: up to 15
repositories with shared branch names and `main` as the usual default. Current
workflow priorities are recorded above; the remaining table preserves original
overnight questions and is not a new request to answer them again.

These are planning assumptions. Update this table and dependent docs when the user answers. Absence of an answer is not permission to install software, start a scheduled run, or publish a release.

Resolved follow-up: the user accepted Tauri/Rust/TypeScript and asked for project-file groups. The plan uses explicit JSON membership with relative paths. React, file format details, fast-forward policy, and release scope remain implementation recommendations. Prioritize the working group workflow when allocating overnight time.

## Other working decisions

5 October 2026 continuation: after the configured-upstream-only recommendation,
the user asked to commit B2 and continue. B3 therefore uses that recommended
first-slice scope: one configured named-remote upstream per attached branch,
no editable destinations, no force or implicit upstream changes. This is the
announced working scope, not a claim that the user separately specified every
edge case. Dirty files stay local; reviewed commit IDs are pinned. Triangular
push-remote overrides differing from the upstream require individual review.
Retries resolve current upstreams into a new confirmation; successful pushes
are never retried implicitly. In-flight pushes may complete after cancellation.

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
