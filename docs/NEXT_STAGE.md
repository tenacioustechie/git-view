# Next feature stage — implementation plan

Started 21 September 2026; product decisions finalized 22 September 2026.
Status: **A1–A4 complete on tested macOS/Windows; the following rebase stage
has a [detailed design](REBASE_DESIGN.md) and source implementation with a
passing macOS gate. Windows acceptance and packages remain pending**.
Implementation resumed after a computer crash on 22 September 2026. A1 passes
its tests, native acceptance, release builds, and package checks at `fe33bf3`;
A2 passes the same gates at `fd637b8`, A3 at `7c0eef5`, and A4 at `9fe0fb8`;
see Build status for evidence. Engineering choices below
are implementation assumptions to verify with fixtures, not additional product
questions or claims of completed functionality.

Baseline: the implemented workflows and outstanding items in
[Feature status](FEATURE_STATUS.md), with application verification at checkpoint
`e044bcbc`. The [User guide](USER_GUIDE.md) describes current behavior. This plan
proposes additions; it does not change their implementation status.

## Outcome and confirmed choices

Help the user understand and resolve complex Git situations: difficult merges,
rebase, and unclear repository state are the main reasons they seek a visual Git
tool. They use both terminal and visual tools for simple operations. The confirmed
sequence remains remote tracking checkout, then cherry-pick, stash, and hunk
staging, with each slice tested and usable before proceeding to the next.
These slice names supplement the original broad M2/M3 roadmap without redefining
its completion.

| Decision | Selected behavior | Status |
| --- | --- | --- |
| Leading track | Everyday Git: remote tracking checkout → cherry-pick → stash → hunk staging | Confirmed by user, 21 September 2026; expanded to include cherry-pick |
| Delivery cadence | Independently usable, tested slices | Confirmed by user, 21 September 2026 |
| Everyday tasks | Push, pull, merge, cherry-pick; uses both terminal and visual tools for simple operations | Confirmed by user |
| Main reason to use a visual Git tool | Complex merges, rebase, and unclear repository state | Clarified by user; no specific defect in current push/pull/merge controls was reported |
| Group size and branch naming | Up to 15 repositories; shared branch names; `main` is the usual default | Confirmed by user, 21 September 2026 |
| Stash restore actions | Apply and keep, plus separately reviewed Delete; no Pop in this slice | Confirmed by user, 21 September 2026 |
| Untracked files in a stash | Explicit inclusion checkbox, initially unchecked; ignored files excluded | Confirmed by user, 21 September 2026 |
| Cherry-pick priority | Immediately after remote tracking checkout | Confirmed by user, 21 September 2026 |
| Rebase timing | Following stage after the four confirmed slices | Confirmed by user, 22 September 2026 |
| First complex-merge improvement | Clear branch/commit context, conflict list, and next-step guidance; files resolved in an external editor | Confirmed by user, 22 September 2026; integrated three-way editing deferred |

Group operations (branch switching, progressive status, then batch push) and
change inspection (side-by-side diffs/navigation, then file history/blame) remain
later candidates. Only the selected first track needs a full implementation
specification now.

The representative user workflow is a group of up to 15 repositories with shared
branch names, normally `main` as the default branch. Use that size for realistic
workflow fixtures alongside the existing 25-repository regression measurement.
This is usage context, not a request to hardcode `main`, change checkout defaults,
or switch all members automatically. Actual Git configuration remains authoritative.

Push, pull, and merge are already implemented for individual repositories, and
groups already support fetch/pull. The user clarified that the need is visual
help for complex operations and ambiguous state; no expansion or repair of those
simple controls is implied. Cherry-pick is absent and confirmed as the second
slice. Rebase follows A4; the first conflict improvement uses external file
resolution with clear visual guidance in Git View.

## Delivery sequence

| Slice | Usable outcome | Required before advancing |
| --- | --- | --- |
| A1: remote tracking checkout | Review an explicit remote branch, create/switch to a local branch, and see its upstream | Branch/config/content assertions, collision and stale-state tests, native Mac/Windows acceptance, local packages, updated guide |
| A2: cherry-pick and conflict guidance | Apply one non-merge commit; understand target/source and stopped state; resolve externally and continue/abort | Success/conflict/abort/empty-state coverage, reload reconciliation, consistent ordinary merge/squash guidance, native acceptance and packages |
| A3: stash | Save changes, inspect, Apply while retaining the stash, or separately Delete; untracked inclusion is opt-in | Content/index preservation, identity/concurrency and conflict tests, native acceptance and packages |
| A4: hunk staging | Stage/unstage supported complete text hunks without changing working-file content | Exact index assertions, stale hunk rejection, mixed staged/unstaged and line-ending cases, native acceptance and packages |
| Following stage: rebase | Plan and implement rebase with understandable state and recovery | [Detailed design](REBASE_DESIGN.md), then a separate implementation and platform gate; interactive rebase and force push are not added by this scheduling decision |

Apply the full completion gate below to each A1–A4 slice before starting the next.
No further product answers are needed to start A1. Record any technical limitation
found in fixtures explicitly; ask a new product question only if it materially
changes an agreed workflow.

## Visual understanding and recovery requirements

Carry these acceptance goals through the selected slices, especially cherry-pick:

- Make the current target branch/HEAD distinct from the commit or history scope
  being inspected. Preview what will change and where before execution.
- Name the actual Git operation in progress. Explain why it stopped: unresolved
  conflicts, an empty result, a hook/signing failure, cancellation, or changed
  external state. When the outcome is uncertain, say so and refresh/inspect it.
- Show affected files and their resolution/staging state, the available next
  action, and why other actions are blocked. Existing files and commits remain
  inspectable where the operation allows it.
- Explain the effects of Continue, Abort, and recovery actions in the context of
  that specific operation, including effects on edits made during resolution.
  Avoid presenting every stopped operation as an ordinary merge.
- After app reload or external Git activity, reconcile the displayed state with
  actual repository state. Do not leave a stale success banner or wrong action
  enabled because an earlier event was missed.
- Native acceptance must exercise a stopped/conflicted workflow as well as a
  successful one, and verify that the displayed explanation and next action match
  the real Git state. Automated assertions are supplemented by a review of whether
  the workflow is understandable without interpreting raw Git output.

Deliver the first shared conflict-guidance improvement with A2: show the current
branch/HEAD, known source commit/ref, operation name, unresolved files, and the
next valid step. If source context cannot be recovered reliably, label it unknown.
Apply consistent explanations to ordinary merge, squash review, and cherry-pick
while preserving their different continue/abort/commit semantics. A3 extends this
guidance to stash conflicts. Users resolve files externally, return, refresh,
stage resolutions, and take the operation-specific next action.

Acceptance includes add/add and modify/delete conflicts as well as ordinary text
conflicts, external file edits, and UI reload. File content heuristics alone must
not claim a conflict is resolved; the displayed state must match Git's index and
operation state. Test that incompatible action buttons stay blocked with reasons.

A full branch-comparison screen and integrated three-way editor are deferred.
Rebase execution belongs to the following stage. A1 retains its focused checkout
scope.

## Existing requirements to carry forward

- Use the established Tauri/Rust/TypeScript application and installed Git.
- Preserve existing edits and normal Git hooks, signing, and credential behavior.
- Keep pull fast-forward-only; dirty group members are skipped. Introducing stash
  must not silently add automatic stashing to pull or branch switching.
- Continue local macOS verification and native Windows verification on
  `git-view-windows` at `C:\codeme\git-view`. Record exact tested revisions.
- Maintain both squash workflows and their recovery/conflict regression coverage.
- Test mutations in disposable repositories and use local test remotes.
- Update user-facing documentation in the implementation change.
- No push or release publication is authorized by this planning work. Distribution
  automation and signing remain separate backlog items.

## Selected track A: everyday Git workflows

### A1 — Check out a remote branch

User outcome: select an existing remote-tracking branch and start working on a
local branch with the intended upstream configured.

Proposed workflow:

1. Select the remote branch and choose a dedicated checkout action.
2. Review the remote/source branch, proposed editable local name, current branch,
   and intended upstream. Fetch remains an explicit action when the list is stale.
3. Create and switch to the local tracking branch after confirming the preview.
4. Refresh branch, history, status, and upstream information.

Proposed first-slice boundaries:

- Require clean status and no Git operation in progress, matching current local
  branch actions.
- Resolve the selected remote explicitly when multiple remotes share a branch name.
- If a local name already exists, offer explicit switching to that local branch
  or choosing a different name; do not reset it or silently change its upstream.
- Surface branches checked out in another worktree as unavailable where applicable.
- Recheck source ref, configuration, branch-name availability, and repository state
  when executing. A changed preview requires review again.

Acceptance: a disposable remote-only feature branch becomes a local branch at
the expected commit, with the exact upstream, and can be used by the existing
pull/push workflow. Test duplicate remote branch names, custom local names,
existing-name collisions, dirty/untracked state, in-progress operations, linked
worktree occupancy, and ref/config changes after preview. Failed or rejected
actions must not overwrite existing branch history or working files. Native UI
acceptance verifies selection → preview → checkout → refreshed upstream.

First-slice implementation assumption: checkout creates and switches to the
tracking branch. Creating one without switching is deferred unless the user's
workflow examples make it necessary. This assumption is not a separately
confirmed product requirement.

#### A1 interaction specification

Proposed labels are design text, not claims about current controls:

1. Add **Check out remote branch** alongside the existing local create/switch
   controls. Choosing a history scope remains read-only.
2. Show an explicit **Remote branch** selector and editable **Local branch name**.
   Suggest the source branch name while preserving user edits to that field.
3. Show the selected remote and intended upstream. An absent branch directs the
   user to the existing Fetch control; opening the form does not contact a server.
4. **Preview checkout** shows the current HEAD, exact source ref and commit,
   local destination name, and upstream to be set. Invalid names and blocked
   repository states get specific reasons.
5. Confirm with **Create and switch**. Prevent duplicate submissions while the
   operation runs, keep errors and activity visible, and refresh repository state
   after every outcome.
6. If the local name already exists, do not reinterpret confirmation as a checkout
   of the remote tip. Offer the existing local-switch workflow separately, showing
   that branch's actual state, or let the user choose a new name.

#### A1 implementation boundaries and dependencies

Extend the typed repository-action/plan flow instead of building another execution
path. Proposed action data: status snapshot ID, backend remote ID, full selected
remote-tracking ref, and local branch name. The backend resolves the exact source
commit and upstream mapping for preview, retains them in the plan, and validates
them under the existing common-repository operation lock before execution.

Do not infer remote ownership by splitting a display label on `/`. The backend
must resolve the configured fetch mapping; exclude symbolic remote HEAD entries
from branch candidates and report ambiguous or unsupported mappings explicitly.
Verify custom mappings and multiple remotes in fixtures before promising support.
Recheck absence of the new local branch as well as source/config/status freshness.

Prefer one Git operation that creates, tracks, and switches, after confirming its
behavior against the supported Git version. Do not append an unguarded upstream
configuration write after checkout. Git failures, hooks, cancellation, and races
can have partial outcomes: inspect and report actual branch/config/worktree state,
without automatic branch deletion, reset, or a blanket rollback promise.

| Owner | A1 work |
| --- | --- |
| Coordinator | Action/selection metadata contracts, IPC integration, fixture agreement, Mac/Windows verification, documentation |
| Git worker | Source/upstream resolution, preflight/execution guards, failure inspection, disposable Git fixtures |
| Repository worker | Checkout form, preview, existing-name conflict flow, component and native workflow checks |
| Group worker | No group implementation needed for A1; available for a bounded review if useful |

#### A1 verification cases

- Exact expected HEAD, new local branch, upstream remote/ref, and clean status
  after success; existing pull and normal push use the intended upstream.
- Two remotes with the same branch name, branch names containing `/`, custom local
  names, invalid names, symbolic remote HEAD, and missing/ambiguous mappings.
- Existing local name, branch checked out in another worktree, source ref moving,
  a local name appearing, or configuration changing between preview and execution.
- Staged, unstaged, untracked, and conflicted files; active merge/squash review;
  detached and unborn starting states, with supported outcomes or explicit reasons.
- Hook failure and cancellation: refresh actual state and report effects honestly;
  preflight rejection must preserve existing branch refs and working-file content.
- Renderer stale-response handling, preserved typed local name, preview cancellation,
  double-submit prevention, keyboard reachability, and retained commit drafts.
- Native happy path on macOS and Windows, plus collision/dirty-state feedback,
  followed by the existing group, commit, merge, both squash, and recovery regressions.

### A2 — Cherry-pick one commit

User outcome: bring one existing commit's changes onto the current branch without
merging the source branch. This slice's position after checkout is confirmed;
the following detailed boundaries are implementation proposals.

Proposed workflow:

1. Select a non-merge commit in history and choose **Cherry-pick commit**.
2. Preview its ID, subject, source changes, and current target branch/HEAD.
   Selecting a history scope or commit never changes the current target branch.
3. Confirm the action. A successful pick produces one new commit with the expected
   changes on the target, while retaining normal hooks/signing behavior.
4. If conflicts occur, display a dedicated cherry-pick state, refresh the affected
   files, resolve externally, and stage resolutions. Offer **Continue cherry-pick**
   and **Abort cherry-pick** with previews and actual state revalidation.

Proposed first-slice boundaries:

- Require a clean local target branch with an existing commit and no operation
  in progress. Source selection is one non-merge commit; multi-commit ranges,
  merge-commit parent selection, and group-wide picks are deferred.
- Preserve the source commit message initially. Do not introduce an automatic
  stash, rewrite existing target commits, or trigger a push.
- Define explicit handling for commits that are initially empty or become empty
  because their changes are already present. Report the condition and offer a
  tested way to finish/exit the operation; never silently invent an empty commit
  or label an unfinished cherry-pick successful.
- Distinguish cherry-pick state and controls from ordinary merge, squash review,
  and stash conflicts. Persist enough operation information to recover the view
  after reload without claiming the app exclusively owns an external Git operation.
- Abort previews explain the effect on resolution edits. Recheck operation
  identity and repository state; do not blindly reset when the state has changed.

Implementation preparation: verify the installed Git's single-pick, hook/signing,
empty-pick, continue/abort, and interruption behavior in disposable fixtures;
then specify typed actions, operation detection, preview guards, and UI states.
Coordinator owns contracts/orchestration, Git worker owns engine/fixtures,
repository worker owns controls and native checks. The group worker has no
required implementation changes for this slice.

Acceptance: expected patch, new parent, message, and author identity on success;
unrelated target changes preserved; clean preflight and stale source/target guards;
conflicts with stage/continue and abort; already-applied/empty source outcomes;
hook/signing failures; cancellation; reload with an operation in progress; and
external Git state changes. Check actual refs, index, files, and operation markers.
Native Mac/Windows checks cover success, conflict resolution/continue, and abort,
and rerun the distinct ordinary merge and both squash workflows for regression.

### A3 — Save and restore work with stash

User outcome: explicitly set work aside, switch tasks, inspect a saved stash, and
restore it without accidentally deleting the saved copy.

Confirmed restore and inclusion behavior, with remaining scope proposed:

- List stashes with message, date, and originating branch information when available.
- Create a named stash of tracked staged/unstaged changes, with an explicit,
  initially unchecked **Include untracked files** choice. Exclude ignored files.
- Inspect changed files and bounded diffs before applying a stash.
- Apply while retaining the stash, with explicit staged-state restoration behavior.
- **Delete** a selected stash as a separate reviewed action. No Pop or bulk-clear
  action in this slice; applying retains the saved stash even after success.
- Do not automatically stash as part of pull/switch/batch actions.

Acceptance: verify saved and restored file contents and staged/unstaged state;
untracked inclusion/exclusion; no-change creation; conflict reporting; retained
stash on application failure; stale list rejection if another tool adds/drops a
stash; linked-worktree sharing; reload during an operation. The selected stash's
identity must remain exact even when list positions change. Conflict handling
must explain external resolution without promising an ordinary merge abort.

The user confirmed apply-and-keep with reviewed Delete and opt-in untracked files.
Remaining implementation assumptions: an explicit, initially unchecked **Restore
staged state** option and a clean destination for applying. These two defaults
are proposals, not separately confirmed user decisions. Creating a stash naturally
operates on dirty files; the clean-destination condition applies only to restoration.

#### A3 interaction and failure specification

Proposed controls: **Stashes**, **Create stash**, **Include untracked files**,
**Apply**, **Restore staged state**, and **Delete**. The list and inspection view
identify the selected stash before offering mutations. Create previews describe
which tracked/untracked changes will be saved and removed from the working tree;
unchecked untracked files remain in place and may keep the repository dirty.

Apply previews identify the saved entry and current target branch, explicitly
state that the stash will remain, and show whether staged state will be restored.
Successful apply refreshes working changes and retains the list entry. A conflict
or failure retains the stash and presents actual file/index state for external
resolution. Do not offer ordinary merge Continue/Abort controls for stash conflicts
unless a genuine ordinary merge is separately in progress.

Delete previews identify the exact entry/message/date and explain that this
removes the saved stash entry without undoing previously applied working changes.
Closing the preview makes no change. A stale selection must never delete a
different entry after another Git process changes the stash list.

Treat stash-entry identity and shared-worktree access as backend concerns. A
numeric list position alone is insufficient, and duplicate stash object IDs need
explicit handling. Define snapshot/revalidation semantics and test concurrent
external additions/deletions before enabling Delete. Refresh after cancellation
or failure and report uncertain outcomes rather than claiming an automatic undo.

Add focused tests for both confirmed defaults: newly created files remain on
disk when inclusion is unchecked, and a successful Apply leaves the stash listed.
The opt-in path must save/restore untracked content; Delete must remove only the
reviewed entry. Exercise staging restoration independently of file content.

### A4 — Stage and unstage individual hunks

User outcome: select a complete change block in a text diff and include only that
block in the next commit.

Proposed initial scope:

- Stage a hunk from an unstaged tracked-text diff and unstage a hunk from a staged
  diff. Keep existing whole-file controls available.
- Bind each action to backend-issued file/hunk identities and the exact diff
  snapshot. The renderer must not submit an arbitrary executable patch.
- Keep working-file content unchanged; change only the intended index content.
- Disable hunk actions with an explanation for unsupported file types, incomplete
  previews, conflicts, and cases whose patch semantics are not yet supported.
- Defer arbitrary line selection. Binary, rename/mode-only, submodule, and
  untracked-file partial staging need explicit scope decisions; whole-file actions
  remain available where supported today.

Acceptance: two separated edits in one file can be split into different commits;
unstaging one hunk preserves other staged content. Verify mixed staged/unstaged
edits, stale content after an external edit, repeated/context-identical hunks,
CRLF/LF, Unicode paths, missing final newline, deletions, and truncated previews.
Assert index/tree contents and unchanged working files, not just exit status.
Native acceptance performs a partial stage, reviews the result, commits, and
checks that the other edit remains uncommitted.

This slice requires structured diff/hunk contracts; the current bounded diff
text preview is not sufficient as an action token. Specify those contracts and
unsupported cases before implementing UI controls.

#### A4 implementation preparation (22 September)

A3 acceptance/packages passed before A4 implementation began. The initial supported
path is complete hunks in existing regular tracked UTF-8 text files with an
unchanged executable mode. Include inserted/deleted lines, emptying an existing
file, CRLF/LF, Unicode paths, missing final newline, and mixed index/worktree
changes. New/deleted files, renames/copies, symlinks, submodules, mode/type changes,
untracked files, unresolved conflicts, intent-to-add/sparse/assume-unchanged
entries, custom filters/encoding/ident conversions, binary data, and incomplete
previews use explicit unavailable reasons and existing whole-file controls.

The working text diff will expose an optional `hunkActions` capability with a
backend diff snapshot ID, staged/unstaged context, preamble, and complete hunk
blocks. Each block has an opaque ID, header, old/new line ranges, and display text.
Actions submit only the diff snapshot, file ID, and hunk ID. Historical and saved
stash previews remain inspection-only. Unchanged working content is verified;
no renderer-supplied patch, path, or line range becomes an execution input.

Preparation probes found that an ordinary text patch can relocate to matching
context, so execution must bind the complete index blob as well as hunk identity.
An internal full-index binary transport patch can provide Git's locked old-blob
check while the user reviews structured text. Force canonical path prefixes;
`diff.noprefix` must never redirect an action to another file. Normal index hooks
remain active, followed by actual index/working-file verification. Unexpected
hook effects are reported without rolling back user changes. These are design
requirements recorded before A4 acceptance, not passing evidence by themselves.

## Candidate track B: groups

| Slice | Proposed behavior | Acceptance focus / decision needed |
| --- | --- | --- |
| B1: explicit group branch switching | Preview matching or selected members and their target local branch; skip dirty, missing-target, unavailable, and incompatible members with reasons | Mixed success/skip/failure, selection fixed during execution, no implicit pull, and no rollback claim across repositories. Decide common branch name versus per-repository targets and whether remote branch creation belongs in this slice. A1 can supply that behavior later. |
| B2: progressive group status | Show completed rows while slower repositories continue; bound concurrent reads; retain errors and navigation | Slow/unavailable fixture, cancellation/stale-generation rejection, selection stability, representative 15-repository usage, 25-repository regression timing, and separate 100-repository stress scheduling. |
| B3: explicit batch push | Review each member's source and destination, then report independent results | No force push; exact destinations; partial remote failures and cancellation; retry after inspection. Decide configured-upstream-only versus editable per-member destinations. |

The original priority of quickly checking/pulling groups remains valid. This
backlog is not prioritized ahead of the confirmed everyday Git stage and following
rebase stage.

## Candidate track C: inspection

| Slice | Proposed behavior | Acceptance focus / decision needed |
| --- | --- | --- |
| C1: side-by-side diffs and navigation | Toggle unified/side-by-side views for working and historical text changes, with clear file and parent context | Insertions/deletions, line alignment, binary/truncated states, keyboard navigation, narrow windows and bounded rendering. Decide whether syntax highlighting is essential initially. |
| C2: file history | Open a selected file's commit history and inspect its changes | Renames, path changes, deleted files, scope boundaries, and bounded paging. Decide initial rename-following expectations. |
| C3: blame | Inspect line attribution and navigate to its commit | Exact selected revision, renamed files, uncommitted state, binary/large-file limits, and attribution without false certainty. |

## Implementation kickoff

1. Read the confirmed choices here and in [Decisions](DECISIONS.md), preserve the
   current working changes, and establish the implementation checkpoint.
2. Finalize A1's typed contracts and fixtures using its user flow, preconditions,
   failure/recovery behavior, ownership, and acceptance cases above. Adopt the
   documented defaults unless technical evidence requires an adjustment.
3. Check required Git behavior against the installed Git and primary documentation
   before choosing commands. Test uncertain behavior in disposable fixtures.
4. Break work along the [documented ownership boundaries](OVERNIGHT.md): coordinator
   owns shared contracts/orchestration/integration; Git worker owns engine/fixtures;
   repository worker owns repository UI; group worker owns group UI. Use only
   workers needed for the selected slice and at most three alongside the coordinator.
5. Implement only after the planning request advances to implementation. Keep
   later feature choices from blocking unrelated first-slice work.

## Completion gate for each delivered slice

- Behavior and failure cases have meaningful backend and renderer tests.
- Typecheck, lint, Rust formatting/Clippy, relevant suites, and existing regression
  checks pass at the integrated source revision.
- New workflow receives native UI/IPC coverage on local macOS and the designated
  Windows checkout. Unrun platform checks remain explicit.
- Build and smoke-test local release packages for the completed delivery slice
  or agreed batch; identify revisions and artifact paths in Build status.
- Update the user guide, feature inventory, affected contracts/specifications,
  and recorded test evidence together. Planning alone does not move a feature
  into the implemented column.
- Report any remaining limitations and the next slice. Do not publish artifacts
  or push branches as part of this gate.
