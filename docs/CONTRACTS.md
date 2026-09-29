# Shared command and data contracts

The coordinator translates these proposed contracts into Rust serde types and TypeScript types during scaffolding, then freezes them before concurrent writes. JSON field names use camelCase. IDs and cursors are opaque strings; timestamps use UTC ISO 8601. Wire enums are tagged unions rather than overloaded nulls.

## Essential data

### File-history inspection (C2)

`get_file_history({ request })` accepts `repositoryId`,
`source: { snapshotId, fileId, path: 'current' | 'previous' }`, and optional
`historySnapshotId` / `cursor`. Source fields reject unknown keys. No display
path or renderer revision expression becomes a Git argument. The backend resolves
raw path bytes from a repository-bound working/commit file snapshot and anchors
the traversal to its recorded HEAD or commit. An unborn HEAD yields an explicit
empty result with `startOid: null`. Missing previous paths are errors.

The response is `{ displayPath, startOid, shallow, history: HistoryPage }`.
History uses `rev-list --full-history --topo-order` with literal pathspecs and
replacement objects disabled, retaining real commit parents. Results are not a
connected DAG and do not automatically follow renames. Git's path traversal can
include descendants if the path was once a directory; the UI explains this.
Pages contain at most 200 commits, have a 30-second total deadline, and use opaque
snapshot-bound cursors. The cache retains at most 32 file-history snapshots;
expired selection/cursor errors require reopening. Repeated valid cursors are
stable against branch movement; shallow ancestry drift and legacy grafts reject
the read. The backend retains loaded OIDs for membership validation.

`get_file_history_commit(repositoryId, historySnapshotId, oid, parentOid)`
accepts only a returned commit in the matching file-history snapshot. Parent
validation is the existing actual-parent check. The returned `CommitDetails`
filters files by raw current/old-path identity, not lossy display text, and its
new opaque file IDs work with existing read-only `get_diff` commit requests.
An unchanged merge-parent comparison can return zero exact-path files. Neither
command observes/reconciles operation journals, refreshes the index, changes
refs, nor invokes a mutation action. Existing preview and missing-object limits
remain authoritative.

```ts
type RepositoryId = string;
type ProjectId = string;
type SnapshotId = string;
type FileId = string;
type Oid = string;

type Head =
  | { kind: 'branch'; name: string; oid: Oid }
  | { kind: 'detached'; oid: Oid }
  | { kind: 'unborn'; name: string };

type GitState = 'idle' | 'merge' | 'squashMerge' | 'rebase' | 'cherryPick' | 'revert' | 'other';

interface OperationContext {
  kind: GitState;
  sourceOids: Oid[];
  sourceRef: string | null;
  originalHead: Oid | null;
  targetBranch: string | null;
  stopReason: 'conflicts' | 'empty' | 'ready' | 'unknown';
  canContinue: boolean;
  canAbort: boolean;
  canSkip: boolean;
  reason: string;
  rebase?: { ontoRef: string | null; ontoOid: Oid | null; currentOid: Oid | null;
    completed: number | null; total: number | null; appOwned: boolean } | null;
}

interface RepositoryStatus {
  repositoryId: RepositoryId;
  snapshotId: SnapshotId;
  observedAt: string;
  head: Head;
  gitState: GitState;
  dirty: boolean;
  counts: { staged: number; unstaged: number; untracked: number; conflicted: number };
  submoduleDirty: boolean;
  upstream: null | { ref: string; remote: string; ahead: number | null; behind: number | null };
  lastSuccessfulFetchAt: string | null;
  activeOperationId: string | null;
  squashMerge?: { originalHead: Oid; sourceOid: Oid; sourceRef: string } | null;
  operationContext?: OperationContext | null;
}

type RepositoryRow =
  | { kind: 'ready'; memberId: string; repositoryId: RepositoryId; label: string; displayPath: string; status: RepositoryStatus }
  | { kind: 'loading'; memberId: string; label: string; displayPath: string }
  | { kind: 'unavailable'; memberId: string; label: string; displayPath: string; error: AppError };

interface ChangedFile {
  fileId: FileId;
  displayPath: string;
  oldDisplayPath: string | null;
  changeKind: 'added' | 'modified' | 'deleted' | 'renamed' | 'copied' | 'typeChanged' | 'unmerged' | 'untracked' | null;
  indexStatus: string | null;
  worktreeStatus: string | null;
  untracked: boolean;
  conflicted: boolean;
  submodule: boolean;
}

interface CommitSummary {
  oid: Oid;
  parents: Oid[];
  subject: string;
  authorName: string;
  authoredAt: string;
  refs: { fullName: string; label: string; kind: 'local' | 'remote' | 'tag' | 'head' }[];
  boundary: boolean;
}

interface HistoryPage {
  snapshotId: SnapshotId;
  commits: CommitSummary[];
  nextCursor: string | null;
}

interface CommitDetails {
  snapshotId: SnapshotId; // context for the returned file IDs and selected parent
  oid: Oid;
  parents: Oid[];
  selectedParent: Oid | null; // null only for a root commit
  message: string;
  author: { name: string; email: string; at: string };
  committer: { name: string; email: string; at: string };
  files: ChangedFile[]; // index/worktree fields null in a historical context
}

type RepositoryAction =
  | { kind: 'stage' | 'unstage'; snapshotId: SnapshotId; fileIds: FileId[] }
  | { kind: 'stageHunk' | 'unstageHunk'; diffSnapshotId: SnapshotId; fileId: FileId; hunkId: string }
  | { kind: 'commit'; statusSnapshotId: SnapshotId; message: string }
  | { kind: 'fetch'; remoteId: string }
  | { kind: 'pull'; statusSnapshotId: SnapshotId }
  | { kind: 'push'; statusSnapshotId: SnapshotId; remoteId: string; destinationRef: string; setUpstream: boolean }
  | { kind: 'createBranch'; statusSnapshotId: SnapshotId; name: string; startOid: Oid; switchAfterCreate: boolean }
  | { kind: 'switchBranch'; statusSnapshotId: SnapshotId; localRef: string }
  | { kind: 'checkoutRemoteBranch'; statusSnapshotId: SnapshotId; remoteId: string; sourceRef: string; name: string }
  | { kind: 'merge' | 'squashMerge'; statusSnapshotId: SnapshotId; sourceRef: string }
  | { kind: 'continueMerge' | 'abortMerge'; statusSnapshotId: SnapshotId }
  | { kind: 'cherryPick'; statusSnapshotId: SnapshotId; sourceOid: Oid }
  | { kind: 'continueCherryPick' | 'abortCherryPick' | 'skipCherryPick'; statusSnapshotId: SnapshotId }
  | { kind: 'startRebase'; statusSnapshotId: SnapshotId; ontoRef: string }
  | { kind: 'continueRebase' | 'skipRebase' | 'abortRebase'; statusSnapshotId: SnapshotId }
  | { kind: 'createStash'; statusSnapshotId: SnapshotId; message: string; includeUntracked: boolean }
  | { kind: 'applyStash'; statusSnapshotId: SnapshotId; stashSnapshotId: SnapshotId; entryId: string; restoreIndex: boolean }
  | { kind: 'deleteStash'; statusSnapshotId: SnapshotId; stashSnapshotId: SnapshotId; entryId: string }
  | { kind: 'finalizeStashDelete'; statusSnapshotId: SnapshotId; recoveryId: string };

type RepositoryActionPlan =
  | { kind: 'eligible'; planId: string; expiresAt: string; summary: string; destination: string | null;
      rebasePreview?: { branch: string; originalHead: Oid; ontoRef: string; ontoOid: Oid;
        baseOid: Oid; commits: { oid: Oid; subject: string }[] } }
  | { kind: 'blocked'; reason: string; error: AppError | null };

interface DiffHunk {
  hunkId: string; header: string;
  oldStart: number; oldLines: number; newStart: number; newLines: number;
  text: string; // complete @@ block, including header
}
type HunkActions =
  | { kind: 'available'; diffSnapshotId: SnapshotId; context: 'staged' | 'unstaged'; preamble: string; hunks: DiffHunk[] }
  | { kind: 'unavailable'; reason: string };
type DiffResult =
  | { kind: 'text'; text: string; truncated: boolean; hunkActions?: HunkActions }
  | { kind: 'binary'; description: string }
  | { kind: 'unavailable'; reason: string };

interface AppError {
  code: string;
  message: string;
  retryable: boolean;
  details?: string; // bounded, redacted diagnostic text
}

type JobState = 'queued' | 'running' | 'succeeded' | 'stopped' | 'failed' | 'skipped' | 'canceled';
interface OperationResult {
  operationId: string;
  batchId: string | null;
  memberId: string | null;
  repositoryId: RepositoryId | null;
  action: string;
  state: JobState;
  startedAt: string | null;
  finishedAt: string | null;
  exitCode: number | null;
  reason: string | null;
  error: AppError | null;
  outputTruncated: boolean;
  outcomeUncertain: boolean;
}
```

Errors such as missing Git, inaccessible repository, stale precondition, authentication failure, lock conflict, invalid project, and unsupported operation get stable codes. Do not infer every failure type from localized stderr. Use exit status plus state inspection; keep a generic Git failure when classification is uncertain.

`RepositoryStatus` is summary data; file lists are separate queries so group polling does not repeatedly send every filename over IPC. A `memberId` identifies an entry within the loaded project revision, including missing entries. It is distinct from a canonical runtime repository ID. Key group rows by member ID. Alias rows may share a repository ID and operation result. Historical files use `changeKind`; working files retain separate index/worktree statuses rather than compressing both into one value.

## Query commands

| Command | Input | Output |
| --- | --- | --- |
| `get_launch_request` / `ack_launch_request` | optional request ID | Queued launch target, ack result |
| `open_repository` | native path selected by launcher/dialog | Repository ID, resolved display path, initial status |
| `discover_repositories` | root path, depth 0–10 | Root/depth, candidates with canonical repository ID and path/label, issues, truncation flag, visited directory count |
| `open_project` | project file path | Project ID, file revision, name, ordered members |
| `save_project` | project ID, expected file revision, version-1 content | New revision or external-change error |
| `create_project` | chosen file path, version-1 content | Project ID and revision |
| `get_status` | repository ID | `RepositoryStatus` |
| `get_changes` | repository ID | Snapshot ID and `ChangedFile[]` |
| `list_refs` | repository ID | Validated local/remote/tag references and HEAD |
| `list_remotes` | repository ID | Display-safe remote IDs/names and resolved defaults |
| `list_remote_branches` | repository ID | `RemoteBranchListing`; local candidates and mapping issues without fetching |
| `list_stashes` | repository ID | `StashList`; exact occurrence IDs, list snapshot, deletion capability/recovery |
| `get_stash_details` | repository ID, list snapshot ID, entry ID, section | `CommitDetails` with file/diff tokens for the saved section |
| `list_rewrites` | repository ID | Durable Combine commits records with reconciled recovery eligibility |
| `get_history` | repository ID, scope; or snapshot ID + cursor | `HistoryPage` |
| `search_history` | repository ID, `{ field: 'message' \| 'author' \| 'oid', text }`, scope, optional snapshot ID/cursor | `HistoryPage` of matches, with original ordered parents |
| `get_file_history` | request with repository ID, source snapshot/file/path choice, optional history snapshot/cursor | `FileHistoryPage`: display path, frozen start OID, shallow flag and history page |
| `get_file_history_commit` | repository ID, file-history snapshot ID, returned OID, optional parent OID | `CommitDetails` filtered to raw current/old-path matches |
| `get_commit` | repository ID, OID, optional parent OID | `CommitDetails`, including file/diff context snapshot |
| `get_diff` | repository ID, snapshot/file ID, staged/unstaged/commit context | `DiffResult` |
| `get_operations` | optional batch ID, cursor | Current snapshots and bounded output |
| `plan_repository_action` | repository ID, `RepositoryAction` | `RepositoryActionPlan`; backend-held preconditions |
| `run_repository_action` | plan ID | Operation ID, current state; revalidate then execute |

Returned file tokens are scoped to their diff/status context. Requesting a commit diff validates that the file and chosen parent belong to that commit query. Invalid/stale snapshots return explicit errors. Commit identity is stable; status identity changes with refreshed observations.

`RemoteBranchListing` contains `targets` and `issues`. Each target has string
fields `remoteId`, `remoteName`, `sourceRef` (full local tracking ref), `sourceOid`,
`upstreamRef` (remote `refs/heads/...`), and `suggestedName`. Each issue has
`sourceRef` and `reason`. The backend resolves configured fetch mappings;
display labels are never parsed to infer remote ownership. Checkout plans bind
the selected source to its OID and revalidate status, configuration, and the
destination name before mutation. A failed hook can follow a completed switch;
the result includes actual-state diagnostics without automatic rollback.

### Stash contracts (A3)

`StashEntry` has `entryId`, `oid`, `message`, and `createdAt` strings, optional
`branch` and `unavailableReason`, and `hasUntracked`. `StashList` has `snapshotId`,
`entries`, optional `deleteUnavailableReason`, and optional `recovery`.
`StashDeleteRecovery` has `recoveryId`, `message`, `canFinalize`, and `backupRef`.
Nullable fields describe unknown context or unavailable actions explicitly.

`get_stash_details` accepts section `worktree` (base to all saved tracked content),
`index` (base to saved index), or `untracked` (empty tree to the optional third
parent). Returned file tokens use the existing commit-context `get_diff` API.
The renderer never sends a stash list number, arbitrary ref expression, or patch
as mutation identity. Backend snapshots bind the common repository identity,
the entire bounded stash ref/reflog state, and the exact selected occurrence;
duplicate object IDs do not become interchangeable entries.

Apply retains the saved entry, with `restoreIndex` separate from file restoration.
Delete removes only the reviewed entry and retains an identified recovery backup.
`finalizeStashDelete` explicitly resumes or finalizes only a reconciled recorded
deletion; list/status reads do not repair it. Unsupported reference backends,
platforms, or external state changes have explicit unavailable/recovery reasons.

Working text diffs optionally return `hunkActions`. Historical/stash inspection
does not issue mutation tokens. Hunk ranges follow Git's one-based/zero-count
conventions and are display metadata only. Actions submit an exact diff snapshot,
file, and hunk identity; backend-owned path/bytes/configuration determine the
candidate index content. A stale source, context, attribute, configuration, or
index requires refresh. Hunk snapshots are bounded to 16 and each source blob or
displayed diff to 2 MiB. Unsupported types/configurations have explicit reasons;
truncated text must never expose executable hunk actions.

Discovery registers repository identities without reading status or changing Git
state. Candidates are proposals; only explicit selection followed by
`create_project` or `save_project` persists membership. CLI discovery requests
use launch kind `discovery`, a root `path` resolved from the invoking cwd, and
optional `scanDepth` (default 3). Launch routing defers acknowledgement while a
DOM or native dialog is open and rechecks navigation after asynchronous loading.

History search uses case-insensitive literal message/author matching and full or
at-least-four-digit hexadecimal commit-ID prefixes. Queries must be nonblank,
single-line, and at most 512 UTF-8 bytes. The backend binds query and scope to
immutable roots; paging cannot change the query or consume a foreign cursor.
OID matches must be commits reachable within those roots. Search returns original
parent IDs; the UI presents a virtualized results list without DAG connectors,
since intervening commits may be omitted. Clear search restores normal history.
Search has a 30-second deadline; overly broad OID prefixes return a refine-query
error rather than a partial result. Historical reads use original stored commit
objects consistently, ignoring replacement refs. Shallow-boundary changes expire
snapshots; legacy grafts require removal or migration before viewing history.

## Mutations and batches

A2 adds four reviewed actions: `cherryPick` binds one full source commit ID,
while `continueCherryPick`, `abortCherryPick`, and `skipCherryPick` bind the
observed operation state. All include `statusSnapshotId`. The optional
`operationContext` identifies known source commits, original HEAD, recorded
target branch, stop reason, and backend-assessed available actions. An empty
source list or null context field means unknown, not permission to infer a source
from selected history. Multiple source OIDs support ordinary merges honestly.

The guided rebase action binds an exact backend-listed local or fetched remote
`ontoRef` and a status snapshot. The eligible plan carries `rebasePreview` with
the original branch/HEAD, exact target tip, merge base, and complete ordered
source commit IDs and subjects. Execution revalidates those values under the
repository lock. Rebase Continue, Skip, and Abort use fresh status snapshots
and backend-assessed capabilities from `operationContext.rebase`. An app-owned
conflict or empty stop returns `stopped` with real operation context; it is not
reported as success. Unknown or externally started rebase states expose no app
mutation controls. The backup and per-worktree record remain internal backend
state. The [rebase design](REBASE_DESIGN.md) records the scope and recovery gate.
Capability flags are explanatory; execution always revalidates the plan.

`plan_repository_action` receives the discriminated action and repository ID and creates a backend-held plan, expiring after five minutes. Record resolved refs/destinations and required HEAD/index/status observations in that plan. `run_repository_action(planId)` revalidates and executes it; submitting a plan twice returns the existing operation. Simple file actions can plan and run from one UI gesture; remote/branch/merge actions show the resolved target where required. M2 adds normal merge, merge continue/abort, and squash-merge variants; return an unsupported error until implemented. No arbitrary argv variant.

Use backend-held preflight tokens for branch/remote operations. Index and worktree observations can race with outside clients; the UI must never promise total exclusion. Report Git's final outcome and refresh. A hook may legitimately alter content during commit.

`plan_batch` accepts project ID/revision, candidate member IDs, action (`fetch` or `pull`), and optional exact branch filter. The backend owns eligibility and returns a plan ID plus all candidate rows with eligible/skipped reason and displayed destination. The plan retains resolved repository identities and preconditions for five minutes; execution still revalidates. Replan after expiry or project edits. Do not make the renderer the sole source of eligibility.

`run_batch(planId)` returns batch and job IDs promptly. Make starting the same plan idempotent so a double click returns the existing batch. Every planned member receives a terminal result, including skips and fetch aliases. `cancel_batch(batchId)` stops queued jobs and requests cancellation of running jobs according to runner support. `retry` creates a new plan, not a replay of stale argv.

## Implemented milestone additions

Implemented M2/M3 additions are defined in `src/api/types.ts` and
`src-tauri/src/contracts.rs`: merge, continue/abort, squash-merge,
`combineCommits { statusSnapshotId, commitOids, message }`, and
`recoverCombine { statusSnapshotId, recoveryId }`. Combine selections are ordered
newest first and validated as a linear suffix ending at the current branch tip.
`RewriteRecord` exposes the phase, original/replacement/backup refs and OIDs,
selected commits, message, error, and `canRecover`. Recovery eligibility is
rechecked from actual Git state when planning and executing; a displayed record
does not authorize a stale restoration. See [recovery design](REWRITE_RECOVERY.md).

Squash-merge exposes `gitState: 'squashMerge'` and optional `squashMerge`
metadata until an explicit reviewed commit completes it. It does not pretend Git
created `MERGE_HEAD` or offer ordinary merge abort. `AppError.exitCode` and
`outputTruncated` preserve bounded process diagnostics across the command boundary.

## Events and fixtures

Subscribe to launch requests, repository invalidation, project-file change, operation progress, and operation completion. Each event carries an ID and monotonic sequence within its stream. Event delivery is an optimization: after reload/reconnect, query current operations and status so missing an event cannot leave a spinner forever.

Before delegation, create shared fixture responses for clean, mixed staged/unstaged, detached, unborn, conflict, missing member, missing upstream, ahead/behind unknown, failed action, binary diff, merge commit, and a paged graph. Keep the same fixture shapes in backend serialization tests and UI mocks. The coordinator reviews contract changes and informs both UI workers before integration.
