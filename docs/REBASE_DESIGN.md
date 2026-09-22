# Rebase workflow design

Prepared 22 September 2026 after the A4 gate and used for the first source
implementation. The starting baseline was code checkpoint `9fe0fb8` and
documentation checkpoint `b9b45ca`; see [Feature status](FEATURE_STATUS.md)
and [Build status](BUILD_STATUS.md) for current implementation and platform
evidence. Planned acceptance cases below remain gates until recorded there.

## Outcome and decisions

Help someone move the current local branch onto a chosen existing branch while
understanding which commits will be replayed, what Git is doing when it stops,
and how to continue or return to the original branch. It is an explicit
single-repository history rewrite. It does not change Pull all, silently fetch,
autostash, force-push, or implement arbitrary interactive rebase.

The user confirmed on 22 September 2026 that the first workflow should allow
any existing local branch or fetched remote branch as the target, without an
automatic fetch, and replay only a linear suffix of the current branch. A merge
commit in that suffix blocks the preview with a clear reason. A separately
reviewed **Skip commit** action is included for a known stopped commit, with a
clear warning that the commit's change will be omitted.

## User workflow

1. On an attached, clean local branch, open **Rebase branch**. Choose **Onto
   branch** from locally recorded local and remote branches. A remote branch
   may be stale; **Fetch** remains a separate reviewed operation. Selecting a
   history scope or a commit does not change the current branch or rebase base.
2. Choose **Preview rebase**. Show the current branch and exact original HEAD,
   chosen ref and exact target tip, merge base, and every commit to replay in
   oldest-first order with subject and full ID. State that those commits receive
   new IDs and normal Push may reject a branch whose old commits were published.
   Remote-tracking refs are hints about publication, not proof of safety.
3. Confirm **Start rebase**. The backend rechecks the branch, both tip IDs,
   merge base, complete commit list, clean index/worktree, effective config,
   and absence of another operation. It creates an app backup ref and durable
   per-worktree record before invoking Git. The result is either completed or
   an explicit stopped state; a canceled/failed command is reconciled against
   actual Git state before the UI describes it.
4. On a stop, show **Rebase in progress**, original branch/HEAD, chosen base,
   current source commit, progress through the reviewed list, unresolved files,
   and the next available action. For rebase conflicts, explain that Git's
   “ours” is the partially rebased series beginning at the chosen base and
   “theirs” is the commit being replayed. Resolve in an external editor, return,
   **Refresh**, and **Stage resolution**. Editing content alone does not clear
   unresolved index entries.
5. **Continue rebase** is available when the backend verifies a known stopped
   commit, no unresolved index entries, and a result ready to commit. It keeps
   Git's current commit message and normal signing/hooks; the app's ordinary
   commit draft remains separate. A Continue may stop at the next commit, so
   refresh progress and source context after every attempt.
6. If a known commit became empty, require an explicit decision. The macOS Git
   2.51.2 disposable probe showed `--empty=stop` pauses, but a subsequent
   `--continue` can still drop that commit. Do not present Continue as a way to
   preserve it. **Skip commit** shows the exact commit and current resolution
   edits in a separate confirmation, warning that the commit's change and those
   edits may be discarded. Recheck identity and working state before invoking
   Git. Preserving an empty commit or editing its message requires an external
   Git workflow in the first slice.
7. **Abort rebase** has a separate preview naming the original branch/HEAD and
   listing current staged, unstaged, conflicted, and untracked state. Explain
   that Git may discard work done during conflict resolution. Refresh and
   inspect actual files/refs afterward; never claim rollback solely from a
   subprocess exit code.

## Eligibility and exact preview

- Require an existing attached local branch, clean index/worktree including
  untracked and submodule changes, and idle Git state. Reject shallow or grafted
  ancestry when the reviewed range cannot be established completely. Replacement
  refs must not change the identity used for preview and execution.
- Accept only an exact `refs/heads/...` or `refs/remotes/...` target emitted by
  the backend. Reject the current branch, tags, symbolic remote HEAD, ambiguous
  refs, missing/non-commit tips, and arbitrary revision expressions.
- Compute one merge base between current HEAD and the target. Multiple bases or
  no shared base require separate handling and are excluded initially. Walk the
  complete `base..HEAD` range in oldest-first order. Reject a merge commit in
  that range and cap the initial preview at 100 commits; report the limit rather
  than truncating the list. Require at least one commit to replay and a target
  beyond the base. No-op/fast-forward cases get an explanation, not a history
  rewrite button.
- Bind the preview to canonical worktree/common Git identity, current branch
  ref and OID, target ref and OID, merge base, exact source commit IDs, status
  snapshot, effective config, and a short expiration. Renderer strings are
  display data, not Git command input. Execution rechecks these values under
  the shared repository mutation lock. External Git writers can still race;
  unexpected ref or file outcomes become manual-review states.
- Before materializing each replayed tree, account for ignored/untracked paths,
  symlink/file ancestors, directory rename destinations, and submodule/gitlink
  changes. Reuse the existing cherry-pick collision policy where valid, and
  test rebase-specific cases in disposable repositories. Conservatively block
  cases whose overwrite risk cannot be bounded in this first slice.

## Git execution and state ownership

The intended non-interactive invocation is structurally:

```text
git --no-replace-objects -c submodule.recurse=false rebase
    --merge --no-fork-point --no-rebase-merges --no-update-refs
    --no-autostash --no-rerere-autoupdate --reapply-cherry-picks
    --keep-empty --empty=stop --onto <reviewed-target-oid> <reviewed-base-oid>
```

Use an argument array, not a shell. The explicit target OID prevents Git from
silently choosing a newly moved target ref after review. Revalidate that ref
immediately before launch and inspect it afterward. `--reapply-cherry-picks`
prevents Git's preliminary duplicate-patch omission; `--empty=stop` makes a
commit that becomes empty visible for review. `--no-update-refs` prevents user
configuration from rewriting other branches, and `--no-autostash` preserves the
clean-start policy. Preserve normal pre-rebase/post-rewrite hooks and signing;
do not pass `--no-verify` or disable them to obtain success. The existing runner
uses `GIT_EDITOR=true`, so Continue accepts Git's current message without a GUI
editor; show that message before Continue. Verify all options on macOS Git
2.51.2 and designated Windows Git 2.53.0 before implementation.

Create a unique backup ref under `refs/git-view/rebase-backups/` pointing to the
original HEAD. Persist a bounded, versioned journal in the worktree Git
directory with the reviewed identities, backup ref, phase, and expected commit
list. Flush before launching Git; retain the backup after completion. Reads do
not repair or remove a journal. Reconcile it against Git's actual rebase
metadata and refs on refresh/reload. `REBASE_HEAD` identifies the current source
commit; the disposable conflict probe also observed `rebase-merge/head-name`,
`orig-head`, `onto`, `stopped-sha`, `done`, and `git-rebase-todo`. Treat these as
version-sensitive evidence: bound and validate each read, test both supported
Git versions, and disable app mutation controls if they disagree with the
recorded plan or were edited externally.

The common-directory app lock serializes Git View mutations across linked
worktrees. Git's own ref/index locks remain authoritative. The app cannot make
external Git writers obey its lock. Do not hold a lock on the source branch
across Git's final ref update. Compare actual branch/HEAD, backup, metadata,
index, worktree, and journal after every subprocess. A nonzero exit with a valid
rebase state is a **stopped** outcome, not completion; a missing/mismatched
state after possible mutation is **uncertain** and requires inspection. No
automatic reset, stash, or retry follows an uncertain outcome.

App-owned rebase records may enable Continue, reviewed Skip, and Abort after a
restart. An externally started rebase receives honest read-only context where
Git metadata is reliable, with instructions to use Git externally for mutation.
If an app record remains but Git reports no matching rebase, show manual review;
only an explicit, narrowly checked record-clearing action may remove a proven
stale marker. Do not infer successful completion or restore the original branch
from a disappeared metadata directory. A retained backup provides a recovery
anchor, but a one-click post-completion reset is outside this first slice because
the old and new trees can differ and later working edits must be protected.

## Contracts and implementation steps

1. Extend the typed action contract with `startRebase`, `continueRebase`,
   `skipRebase`, and `abortRebase`. The start action identifies an
   exact backend ref and status snapshot. Extend the eligible plan with a typed
   rebase preview rather than encoding commit IDs only in summary prose. Keep
   the plan ID opaque and revalidate all previewed identities on execution.
2. Add bounded rebase progress to `operationContext`: original branch/HEAD,
   onto ref/OID, current source commit, completed/total counts, stop reason,
   and whether the record is app-owned. Distinguish completed, stopped,
   rejected-before-mutation, and uncertain action results. A shared `stopped`
   job state is preferable if it can be integrated without changing existing
   batch-result interpretation.
3. Implement backend preview, durable record/backup, execution, status
   reconciliation, Continue/Skip/Abort guards, and collision checks in
   `src-tauri/src/git/`. Keep command registration and shared wire contracts
   in their existing coordinator-owned files.
4. Add a repository rebase panel and operation-specific conflict guidance in
   `src/features/repository/`. Use the existing action confirmation pattern;
   preserve history inspection and the separate normal commit draft while
   stopped. Fetch remains available where safe; incompatible mutations have
   visible reasons. The UI must ignore a delayed preview after a branch/ref or
   status change.
5. Update the User guide, Feature status, affected contracts/engine docs,
   native fixture, and Build status with actual evidence. Keep implementation
   controls unavailable until the complete slice passes its platform gate.

## Acceptance gate

- Real temporary repositories: exact linear replay order, final parent/tree,
  source metadata, unchanged target ref, backup ref, and unchanged unrelated
  refs; local and remote-tracking targets; SHA-1 and SHA-256 where supported.
- Reject dirty/untracked/submodule state, detached/unborn HEAD, existing Git
  operations, merge commits in the replay range, shallow/ambiguous ancestry,
  symbolic or moved targets, changed source/config, and incomplete previews
  before mutation. Test linked worktrees and external ref/index races.
- Exercise text, add/add, modify/delete, directory rename, ignored-path, and
  symlink-ancestor conflicts. Verify exact index stages and working bytes;
  stage a resolution externally, Continue through a second stopped commit, and
  Abort after resolution edits. Test stopped empty/duplicate commits and the
  reviewed Skip consequence, including resolution edits made before Skip.
- Exercise pre-rebase hook rejection, commit hook/signing failure, post-rewrite
  side effects, cancellation, timeout, app reload during a running rebase, and
  externally changed/deleted rebase metadata. Assert actual refs/files/backup
  after every case and retain evidence on uncertain outcomes.
- Renderer tests cover preview identity, warning text, target/source clarity,
  reversed ours/theirs explanation, conflict list, action disablement, stale
  async results, and operation recovery after remount. Native macOS and Windows
  checks use real IPC/UI to run one clean rebase and one stopped conflict through
  Continue and reviewed Abort/Skip, then verify final Git objects and bytes.
- Run the existing full frontend/backend, typecheck, lint, formatting, strict
  Clippy, native smoke, release/package and installer gates on both designated
  platforms. Record exact revisions and retain previous A4 artifacts. Linux and
  Intel Mac checks remain unverified until run.

## Sources and preparation evidence

- [Git rebase reference](https://git-scm.com/docs/git-rebase): mode/options,
  empty-commit and duplicate-patch behavior, reversed conflict sides, and
  cleanup semantics.
- [Git hooks reference](https://git-scm.com/docs/githooks): pre-rebase and
  post-rewrite hooks remain part of the operation.
- Disposable local Git 2.51.2 probes on 22 September 2026 confirmed the
  conflict stop metadata/`REBASE_HEAD`, staged-resolution Continue, and the
  empty-stop behavior described above. These are design evidence, not app
  acceptance or Windows behavior evidence.
