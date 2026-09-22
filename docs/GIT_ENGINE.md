# Git engine behavior

These are engine policies and command strategies. [Feature status](FEATURE_STATUS.md)
distinguishes implemented behavior from remaining roadmap work; [Build status](BUILD_STATUS.md)
records actual verification. Test new commands against supported Git versions and
disposable fixtures before wiring them to UI actions.

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
| Stage/unstage hunk (A4) | Backend-issued complete text block, exact index/worktree/config/attribute snapshot, index-only mutation, stale rejection and post-hook result inspection |
| Commit | Nonblank message; staged content for normal commit; message through stdin; retain draft on failure, refresh actual index/worktree, no automatic rollback; normal hooks/signing |
| Fetch | Explicit remote selection; default upstream remote, else `origin`, else the sole remote; otherwise request a choice; no automatic prune or recursive submodule fetch |
| Pull | Attached branch, upstream, clean status, no ongoing operation; explicit fast-forward-only, rebase disabled, autostash disabled |
| Push | Show resolved remote and destination ref; push current branch to that one ref explicitly; explicitly disable follow-tags and recursive submodule push; no mirror/force; first push chooses remote/ref and optional upstream |
| Create/switch branch | Validate name and resolve target; initial policy requires clean status and no ongoing operation; backend rechecks HEAD |
| Remote tracking checkout | Resolve explicit fetch mapping, require clean idle state and unused local name/config namespace, pin reviewed source during execution, create/switch with direct tracking, preserve ignored files, inspect actual post-hook state |
| Cherry-pick (A2) | One full non-merge commit ID, clean existing attached target, source/target preview, ignored-content protection, persistent stopped-state context, reviewed Continue/Abort/empty Skip |
| Stash (A3) | Named Create with opt-in untracked files; bounded inspection; clean-destination Apply retaining entry; separately reviewed exact-entry Delete with backup and explicit interrupted-deletion recovery |
| Merge (M2) | Clean starting state; show source and current target; use normal merge, allow fast-forward, avoid editor wait; support conflicts and hook failures |
| Squash-merge (M2) | Clean starting state; merge source with `--squash`; leave changes staged for review and an explicit normal commit |

Pull should be equivalent to `git -c pull.rebase=false -c rebase.autoStash=false -c merge.autoStash=false pull --ff-only --no-rebase --no-autostash --no-recurse-submodules`, with resolved upstream semantics validated in fixtures. Fetch also explicitly passes `--no-recurse-submodules`. Explicit policies prevent user pull settings from unexpectedly rebasing or stashing during a batch. Divergence fails for that repository; it is not automatically merged. See [git-pull](https://git-scm.com/docs/git-pull).

Before push, resolve the actual configured push destination, including push remote rules, rather than assuming upstream and push target coincide. Reject mirror/all-ref destinations and multiple effective push URLs in M1 with an explanation. Previewed targets must match the executed explicit remote/refspec. Pass `--no-follow-tags --recurse-submodules=no` so user configuration cannot publish additional tags or submodule repositories. Normal Git hooks and remote rejection remain authoritative. See [git-push](https://git-scm.com/docs/git-push).

An ordinary merge can leave a merge-in-progress state and support Continue or Abort. A squash merge does not create `MERGE_HEAD`, so it cannot reuse ordinary merge abort semantics. For M2 expose staged review and external/manual cleanup instructions; no automatic destructive rollback. Record the pre-operation HEAD and surface conflicts. See [git-merge](https://git-scm.com/docs/git-merge).

## Remote tracking checkout

Candidate enumeration reverses configured fetch refspecs and excludes symbolic
refs, non-commit targets, negative mappings, and ambiguous ownership/destinations.
Explicit source/remote selection does not fetch. Checkout rechecks status/config,
rejects existing branch/config namespaces, and holds a Git reference transaction
on the reviewed source through the switch and hooks. It uses direct tracking,
no guessing/submodule recursion, and preserves ignored files that would collide.
Post-checkout inspection verifies HEAD, local ref, upstream config, and clean
state. Hooks can leave partial effects; errors describe observed state without
resetting or deleting branches. See [Git switch](https://git-scm.com/docs/git-switch)
and [Git branch tracking](https://git-scm.com/docs/git-branch).

## Cherry-pick and operation context

A2 starts one non-merge commit with `git --no-replace-objects cherry-pick --no-edit
-- <oid>`. Initially empty sources are rejected; Git's default stop on a
becomes-empty result leaves explicit Skip or Abort. Continue uses Git's sequencer
continuation rather than a normal app Commit, preserving the source message and
author. Normal signing remains active. Initial cherry-pick invokes Git's native
prepare-commit-msg/post-commit hooks; Continue also invokes normal commit hooks.
No extra hook invocation or hook/signing bypass is added. See
[Git cherry-pick](https://git-scm.com/docs/git-cherry-pick/2.50.0).

Cherry-pick and its continuation/exit commands explicitly set
`submodule.recurse=false`, as does ordinary merge Abort, to avoid discarding
private submodule-worktree edits during recovery. They do not implement submodule
lifecycle management.

A private per-worktree `git-view-cherry-pick.json` records the reviewed branch,
original HEAD, source, and operation identity before mutation. Stopped state is
reconciled against actual Git metadata on refresh/reload. A missing/mismatched
marker or moved target is not guessed to be completed or rolled back. A clean,
unchanged target with only an interrupted app marker can explicitly clear that
marker; other inconsistent cases require manual review. External single picks
retain unknown original context; external multi-pick sequencers are excluded.

Git can overwrite ignored paths during a cherry-pick, including directory-rename
destinations even on a conflict. Preflight and execution check local destination
collisions. Potential directory relocation with ignored content is conservatively
blocked. This does not claim exclusive access against external Git processes or
arbitrary user hooks/merge drivers. Abort/Skip previews explain their effect on
staged resolution edits; no automatic reset or rollback is performed.

`operationContext` carries known source IDs, original target information, stop
reason, and backend capabilities for cherry-pick, ordinary merge, and squash
review. Conflict completion follows Git index state, not a search for text
markers. Missing original context is displayed as unknown.

## Guided rebase

The first rebase workflow takes an exact local or fetched remote ref and a clean
attached branch. Preview records one merge base and the complete linear source
range, capped at 100 commits; merge commits, shallow ancestry, grafts, and
replacement refs are rejected. A non-interactive `git rebase --merge --onto`
uses the reviewed target/base OIDs, with autostash and automatic other-ref
updates disabled. `--reapply-cherry-picks --empty=stop` makes duplicates and
becomes-empty commits available for explicit review. Normal hooks and signing
remain active. Pull still never rebases automatically.

Before Git starts, the engine creates a unique backup under
`refs/git-view/rebase-backups/` and writes `git-view-rebase.json` in the
worktree Git directory. Refresh reconciles the record with actual rebase
metadata and `REBASE_HEAD`; Continue, Skip, and Abort are enabled only for a
matching app-owned stop. An external rebase is inspectable but read-only in the
app. Skip names the current commit and warns that its change and resolution
edits can be lost. Abort may discard resolution edits while restoring the
original branch. Unexpected metadata/ref outcomes remain for manual review;
there is no automatic reset. The backup is retained after completion. The
[rebase design](REBASE_DESIGN.md) gives command options and acceptance cases.

## Complete text hunks

A4 extends working text diffs with bounded, backend-issued hunk identities.
Display headers/ranges are not execution input. Bind each reviewed block to its
canonical repository/worktree, raw path, diff context, exact index blob/mode,
index evidence, raw working bytes, HEAD/operation context, effective attributes,
and configuration. Require idle Git state and a supported regular tracked file;
whole-file conflict resolution remains a separate existing operation.

The canonical text diff has fixed `a/` and `b/` prefixes, complete hunks, and no
external diff, text conversion, relative-path redirection, or rename guessing.
Construct the selected candidate index blob from the exact reviewed byte range;
Unstage reverses that block against the reviewed staged content. Internal binary
transport carries the original full blob identity so Git's native index-locked
application rejects a changed preimage rather than relocating text to similar
context. Its generator uses an isolated temporary repository with user config,
hooks, templates, and conversions excluded. Actual target index mutation keeps
normal hooks. See [Git apply](https://git-scm.com/docs/git-apply/2.51.0).

Verify resulting index entries and unchanged working bytes after application.
Unexpected hooks/external changes are uncertain outcomes; do not undo them.
Never execute renderer-supplied patch text. Keep at most 16 hunk snapshots and
bound each source blob/displayed diff to 2 MiB, without duplicating candidate
blobs for every hunk.

Initial support includes line insertions/deletions, emptying existing files,
LF/CRLF, Unicode paths, missing final newline, repeated-context blocks, and mixed
staged/unstaged edits. New/deleted files, renames/copies, mode/type changes,
symlinks, submodules, binary or invalid-UTF-8 content, conflicts, special index
flags, unsupported filters/encoding/ident conversions, and incomplete previews
have explicit unavailable reasons. Historical and stash diffs remain read-only.
See Build status for the implementation gate; this policy is not acceptance
evidence by itself.

## Stashes

Stash entry identities bind a common Git directory, the complete bounded stash
ref/reflog snapshot, and the selected occurrence. Duplicate object IDs are not
interchangeable entries. Linked worktrees share the stash list and application
mutation lock. Refresh invalidates changed list selections; no positional
`stash@{n}` from the renderer is executed as an unchecked mutation target.

Create saves tracked index/worktree changes with a nonblank, single-line message
of at most 4,096 UTF-8 bytes. `--include-untracked` is explicit and initially off;
ignored files remain excluded. No eligible changes produces an explanation,
not a false successful creation. Changed submodules/gitlinks are rejected.
Apply requires clean idle state and an existing HEAD, pins the reviewed object,
and explicitly chooses `--index` or `--no-index` rather than inheriting
`stash.index`. Default Apply can still stage newly added files. Neither success
nor failure invokes Pop or Delete. Git can partially apply changes before an
error, so failures refresh the actual index/worktree and do not automatically
roll back. Both actions disable recursive submodule updates; Apply rejects saved
gitlink changes and ignored-content collision/directory-rename hazards.
See [Git stash](https://git-scm.com/docs/git-stash/2.51.0).

Inspection compares the saved base against tracked WIP or saved index; the
untracked section compares the optional third-parent tree against an empty tree.
File identities and bounded historical diff handling are reused. Generic index
conflicts without Git operation markers retain unknown source context and use
external resolution, staging, and ordinary commit. They do not receive ordinary
merge Continue/Abort controls.

Delete initially supports files ref storage on macOS and Windows. It holds Git's
reference transaction lock, revalidates the exact ref/reflog, retains the selected
object under `refs/git-view/stash-deletions`, and journals the intended old/new
metadata pair before installation. Multi-entry removal preserves other entries'
order and metadata while repairing the old-object chain; singleton removal uses
Git's prepared compare-and-swap reference deletion. Platform replacement requires
the existing destination, avoiding recreation after an external whole-log drop.
Uncertain installation outcomes retain recovery data. Opening/status/list reads
never repair the journal. Explicit Finalize accepts only recorded metadata pairs;
unrelated external state requires manual review. Backups remain after success.
This does not exclude arbitrary direct filesystem writes or user hook effects.

The initial list limit is 1,000 entries and 8 MiB of reflog data. Reftable storage
is unsupported. Linux Delete has no implemented atomic replacement primitive;
list/Create/Apply are separate from that platform limitation. Native Linux
execution remains unverified.

## Group pull and batching

Pull all snapshots all filtered group members; Pull selected snapshots explicit checked entries. With a branch filter, match the exact current local branch name. Without it, each repository pulls its own current branch's upstream. Do not switch branches or assume all upstream branches have the same name.

Preview each candidate with eligibility, current branch/HEAD/upstream, and reason if skipped: dirty, conflict/in-progress, different branch, detached/unborn, no upstream, inaccessible, unsupported, or busy. Dirty repositories are skipped for manual attention, as requested by the user. A preview is an estimate; remote availability/divergence may only be known after fetch.

Execution acquires the shared-repository mutation lock, then rechecks branch, HEAD, upstream, and status. A stale target is skipped with `precondition_changed`, never silently reinterpreted. Run up to three independent network jobs. Git's own locks remain necessary because external clients can race between checks and execution.

Every candidate gets a result. Completed pulls remain completed if another repository fails. Retry only selected failures through a fresh plan. Cancel stops queued jobs; running jobs may complete and must be reported accurately. A timeout or interrupted push is never presented as proof that the remote was unchanged. Refresh after each outcome and show a final count by result state.

Fetch can operate on dirty worktrees. Coalesce fetches only for shared Git directories with identical resolved remote requests; give each member a result. Pulls for separate worktrees remain separate, serialized jobs.

## Existing-commit squash: later design gate

Combining existing commits is a confirmed requirement alongside squash-merge, scheduled for M3. Restrict the first version to at least two commits forming a contiguous, linear suffix ending at HEAD, with a parent before the suffix, no merge commits, a clean index/worktree, and an attached branch. Show exactly which commits will be replaced and let the user edit the combined message. Avoid claiming a commit is definitely unpublished based only on possibly stale remote-tracking refs.

Require an application backup ref and a recoverable operation journal before rewriting. Preserve original HEAD and the intended replacement tree/message. Design failure/interruption recovery and test hook/signing rejection before enabling the action. A casual soft-reset-then-commit sequence is insufficient. Arbitrary historical squash, root squash, published-history force push, and general interactive rebase remain separate follow-ups.
