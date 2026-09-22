# Git engine behavior

These are implementation requirements and proposed command strategies, not code already present. Test commands against the chosen minimum Git version and local fixtures before wiring them to UI actions.

## Runner and identity

One backend runner invokes an executable and argument array with an explicit cwd. No shell interpolation. Use typed actions and backend-held repository/file tokens. Resolve root, worktree Git directory, and common Git directory through Git, recognizing `.git` files and linked worktrees. See [git-rev-parse](https://git-scm.com/docs/git-rev-parse).

Clear inherited repository redirection variables, including `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_COMMON_DIR`, `GIT_OBJECT_DIRECTORY`, and alternate-object overrides. Preserve normal user config, SSH agent, credentials, signing, and hooks. Avoid shell-based PATH discovery. Disable pagers/color for machine reads. Use `GIT_OPTIONAL_LOCKS=0` for background reads only. Never auto-delete Git lockfiles or change `safe.directory`.

Use `GIT_TERMINAL_PROMPT=0` and no stdin terminal for GUI jobs. Existing helpers may supply credentials; SSH or signing prompts can still need special handling. Bound job duration, show authentication failure with useful context, and keep helper/browser interaction visible when supported. Do not replace user credentials or disable signing/hooks to make a failed operation succeed. SSH host-key and encrypted-key flows require smoke tests; avoid silent trust changes.

Drain stdout/stderr concurrently to avoid deadlock. Read commands may be canceled/restarted; mutation timeout or cancellation is an uncertain outcome until refreshed. Bound retained diagnostic output to 512 KiB per operation initially, with a truncation flag. This cap does not silently truncate machine-readable status/history: stream-parse complete records or return an explicit output-limit error. Never interpret partial structured output as complete or clean. Never log credential-bearing remote URLs unredacted.

## Status and files

Use `git status --porcelain=v2 --branch -z --untracked-files=all --ignore-submodules=none`. Parse NUL records as bytes, recognize ordinary/rename/unmerged/untracked records and extensible headers. Renames include a second pathname. Split fixed metadata fields only; paths can contain whitespace and newlines. See [git-status](https://git-scm.com/docs/git-status).

Model index and working-tree changes separately, conflict counts, submodule indicators, unborn/detached/branch HEAD, upstream, and optional ahead/behind values. Missing counts are unknown. Compute dirty from staged, unstaged, untracked, conflict, or submodule changes; ignored files do not count. Status errors are explicit unavailable states.

Detect merge, rebase, cherry-pick, revert, and sequencer state using paths resolved by Git. Do not assume all metadata lives at `<root>/.git`. M1 displays ongoing operations and blocks conflicting actions; M2 adds ordinary merge completion/abort. Allow refresh and fetch where appropriate.

Map raw paths to opaque file IDs scoped to the current repository snapshot. Display decoding is separate from execution. Treat supplied paths literally, including pathspec-looking names; use Git's literal pathspec mechanism and `--`/NUL pathspec input where supported. Handle Windows command length limits. Reject stale file IDs and refresh instead of targeting a newly selected path by accident.

## History and diffs

Snapshot explicit selected ref tip OIDs plus detached HEAD. Traverse ordered parents using a machine-readable format and topological order. OIDs are opaque strings; do not assume SHA-1 length. Stop conditions include unborn history, shallow boundaries, missing objects, and invalidated snapshots.

Read commit metadata in batches with unambiguous framing; `cat-file --batch` supplies object type/size before the body. Check size limits and partial reads. Commit messages, author data, and legacy encodings need display decoding independent of object identity. See [git-cat-file](https://git-scm.com/docs/git-cat-file).

Get changed paths from NUL-delimited metadata rather than patch headers. Use index-vs-HEAD for staged diffs and worktree-vs-index for unstaged diffs. Untracked files require a bounded new-file preview. Historical diffs compare commit to selected parent, defaulting to first parent; root commits compare with an empty tree. Disable external diff and textconv. Return binary/too-large/unavailable states and cap textual previews at 2 MiB initially. Never stream arbitrary file content into HTML.

## Operation policies

| Action | Preconditions and behavior |
| --- | --- |
| Stage files | Stage exactly selected full files, including deletion/rename semantics; never silently stage the whole repository |
| Unstage files | Restore selected index entries from HEAD; support unborn repos by removing only index entries while preserving working files |
| Commit | Nonblank message; staged content for normal commit; message through stdin; retain draft on failure, refresh actual index/worktree, no automatic rollback; normal hooks/signing |
| Fetch | Explicit remote selection; default upstream remote, else `origin`, else the sole remote; otherwise request a choice; no automatic prune or recursive submodule fetch |
| Pull | Attached branch, upstream, clean status, no ongoing operation; explicit fast-forward-only, rebase disabled, autostash disabled |
| Push | Show resolved remote and destination ref; push current branch to that one ref explicitly; explicitly disable follow-tags and recursive submodule push; no mirror/force; first push chooses remote/ref and optional upstream |
| Create/switch branch | Validate name and resolve target; initial policy requires clean status and no ongoing operation; backend rechecks HEAD |
| Remote tracking checkout | Resolve explicit fetch mapping, require clean idle state and unused local name/config namespace, pin reviewed source during execution, create/switch with direct tracking, preserve ignored files, inspect actual post-hook state |
| Merge (M2) | Clean starting state; show source and current target; use normal merge, allow fast-forward, avoid editor wait; support conflicts and hook failures |
| Squash-merge (M2) | Clean starting state; merge source with `--squash`; leave changes staged for review and an explicit normal commit |

Pull should be equivalent to `git -c pull.rebase=false -c rebase.autoStash=false -c merge.autoStash=false pull --ff-only --no-rebase --no-autostash --no-recurse-submodules`, with resolved upstream semantics validated in fixtures. Fetch also explicitly passes `--no-recurse-submodules`. Explicit policies prevent user pull settings from unexpectedly rebasing or stashing during a batch. Divergence fails for that repository; it is not automatically merged. See [git-pull](https://git-scm.com/docs/git-pull).

Before push, resolve the actual configured push destination, including push remote rules, rather than assuming upstream and push target coincide. Reject mirror/all-ref destinations and multiple effective push URLs in M1 with an explanation. Previewed targets must match the executed explicit remote/refspec. Pass `--no-follow-tags --recurse-submodules=no` so user configuration cannot publish additional tags or submodule repositories. Normal Git hooks and remote rejection remain authoritative. See [git-push](https://git-scm.com/docs/git-push).

An ordinary merge can leave a merge-in-progress state and support Continue or Abort. A squash merge does not create `MERGE_HEAD`, so it cannot reuse ordinary merge abort semantics. For M2 expose staged review and external/manual cleanup instructions; no automatic destructive rollback. Record the pre-operation HEAD and surface conflicts. See [git-merge](https://git-scm.com/docs/git-merge).

## Group pull and batching

Pull all snapshots all filtered group members; Pull selected snapshots explicit checked entries. With a branch filter, match the exact current local branch name. Without it, each repository pulls its own current branch's upstream. Do not switch branches or assume all upstream branches have the same name.

Preview each candidate with eligibility, current branch/HEAD/upstream, and reason if skipped: dirty, conflict/in-progress, different branch, detached/unborn, no upstream, inaccessible, unsupported, or busy. Dirty repositories are skipped for manual attention, as requested by the user. A preview is an estimate; remote availability/divergence may only be known after fetch.

Execution acquires the shared-repository mutation lock, then rechecks branch, HEAD, upstream, and status. A stale target is skipped with `precondition_changed`, never silently reinterpreted. Run up to three independent network jobs. Git's own locks remain necessary because external clients can race between checks and execution.

Every candidate gets a result. Completed pulls remain completed if another repository fails. Retry only selected failures through a fresh plan. Cancel stops queued jobs; running jobs may complete and must be reported accurately. A timeout or interrupted push is never presented as proof that the remote was unchanged. Refresh after each outcome and show a final count by result state.

Fetch can operate on dirty worktrees. Coalesce fetches only for shared Git directories with identical resolved remote requests; give each member a result. Pulls for separate worktrees remain separate, serialized jobs.

## Existing-commit squash: later design gate

Combining existing commits is a confirmed requirement alongside squash-merge, scheduled for M3. Restrict the first version to at least two commits forming a contiguous, linear suffix ending at HEAD, with a parent before the suffix, no merge commits, a clean index/worktree, and an attached branch. Show exactly which commits will be replaced and let the user edit the combined message. Avoid claiming a commit is definitely unpublished based only on possibly stale remote-tracking refs.

Require an application backup ref and a recoverable operation journal before rewriting. Preserve original HEAD and the intended replacement tree/message. Design failure/interruption recovery and test hook/signing rejection before enabling the action. A casual soft-reset-then-commit sequence is insufficient. Arbitrary historical squash, root squash, published-history force push, and general interactive rebase remain separate follow-ups.
