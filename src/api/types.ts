export type RepositoryId = string;
export type ProjectId = string;
export type SnapshotId = string;
export type FileId = string;
export type Oid = string;

export type Head =
  | { kind: 'branch'; name: string; oid: Oid }
  | { kind: 'detached'; oid: Oid }
  | { kind: 'unborn'; name: string };

export type GitState = 'idle' | 'merge' | 'squashMerge' | 'rebase' | 'cherryPick' | 'revert' | 'other';

export interface OperationContext {
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
  rebase?: { ontoRef: string | null; ontoOid: Oid | null; currentOid: Oid | null; completed: number | null; total: number | null; appOwned: boolean } | null;
}

export interface RepositoryStatus {
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
  squashMerge?: { originalHead: string; sourceOid: string; sourceRef: string } | null;
  operationContext?: OperationContext | null;
}

export type RepositoryRow =
  | { kind: 'ready'; memberId: string; repositoryId: RepositoryId; label: string; displayPath: string; status: RepositoryStatus }
  | { kind: 'loading'; memberId: string; label: string; displayPath: string }
  | { kind: 'unavailable'; memberId: string; label: string; displayPath: string; error: AppError };

export interface ChangedFile {
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

export interface CommitSummary {
  oid: Oid;
  parents: Oid[];
  subject: string;
  authorName: string;
  authoredAt: string;
  refs: { fullName: string; label: string; kind: 'local' | 'remote' | 'tag' | 'head' }[];
  boundary: boolean;
}

export interface HistoryPage {
  snapshotId: SnapshotId;
  commits: CommitSummary[];
  nextCursor: string | null;
}
export interface HistorySearch { field: 'message' | 'author' | 'oid'; text: string }
export interface FileHistorySource { snapshotId: string; fileId: string; path: 'current' | 'previous' }
export interface FileHistoryRequest { repositoryId: string; source: FileHistorySource; historySnapshotId?: string | null; cursor?: string | null }
export interface FileHistoryPage { displayPath: string; startOid: string | null; shallow: boolean; history: HistoryPage }

export interface CommitDetails {
  snapshotId: SnapshotId; // context for the returned file IDs and selected parent
  oid: Oid;
  parents: Oid[];
  selectedParent: Oid | null; // null only for a root commit
  message: string;
  author: { name: string; email: string; at: string };
  committer: { name: string; email: string; at: string };
  files: ChangedFile[]; // index/worktree fields null in a historical context
}

export type RepositoryAction =
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
  | { kind: 'finalizeStashDelete'; statusSnapshotId: SnapshotId; recoveryId: string }
  | { kind: 'combineCommits'; statusSnapshotId: SnapshotId; commitOids: string[]; message: string }
  | { kind: 'recoverCombine'; statusSnapshotId: SnapshotId; recoveryId: string };

export type RepositoryActionPlan =
  | { kind: 'eligible'; planId: string; expiresAt: string; summary: string; destination: string | null; rebasePreview?: { branch: string; originalHead: Oid; ontoRef: string; ontoOid: Oid; baseOid: Oid; commits: { oid: Oid; subject: string }[] } }
  | { kind: 'blocked'; reason: string; error: AppError | null };

export interface DiffHunk {
  hunkId: string; header: string;
  oldStart: number; oldLines: number; newStart: number; newLines: number;
  text: string;
}
export type HunkActions =
  | { kind: 'available'; diffSnapshotId: SnapshotId; context: 'staged' | 'unstaged'; preamble: string; hunks: DiffHunk[] }
  | { kind: 'unavailable'; reason: string };
export type DiffResult =
  | { kind: 'text'; text: string; truncated: boolean; hunkActions?: HunkActions }
  | { kind: 'binary'; description: string }
  | { kind: 'unavailable'; reason: string };

export interface AppError {
  code: string;
  message: string;
  retryable: boolean;
  details?: string; // bounded, redacted diagnostic text
  exitCode?: number;
  outputTruncated?: boolean;
}

export type JobState = 'queued' | 'running' | 'succeeded' | 'stopped' | 'failed' | 'skipped' | 'canceled';
export interface OperationResult {
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

export interface RepositoryOpen { repositoryId: string; displayPath: string; status: RepositoryStatus }
export interface Changes { snapshotId: string; files: ChangedFile[] }
export interface GitRef { fullName: string; label: string; oid: string; kind: 'local' | 'remote' | 'tag' | 'head' }
export interface Remote { id: string; name: string; defaultFetch: boolean; defaultPush: boolean; destinationRef: string | null }
export interface RemoteCheckoutTarget { remoteId: string; remoteName: string; sourceRef: string; sourceOid: string; upstreamRef: string; suggestedName: string }
export interface RemoteBranchIssue { sourceRef: string; reason: string }
export interface RemoteBranchListing { targets: RemoteCheckoutTarget[]; issues: RemoteBranchIssue[] }
export interface StashEntry {
  entryId: string;
  oid: Oid;
  message: string;
  createdAt: string;
  branch: string | null;
  hasUntracked: boolean;
  unavailableReason: string | null;
}
export interface StashDeleteRecovery { recoveryId: string; message: string; canFinalize: boolean; backupRef: string }
export interface StashList { snapshotId: SnapshotId; entries: StashEntry[]; deleteUnavailableReason: string | null; recovery: StashDeleteRecovery | null }
export type StashSection = 'worktree' | 'index' | 'untracked';
export interface ProjectContent { version: 1; name: string; repositories: { path: string; label?: string }[] }
export interface Project { projectId: string; revision: string; displayPath: string; name: string; content: ProjectContent; members: RepositoryRow[]; stale: boolean; error: AppError | null }
export interface BatchRequest { projectId: string; revision: string; memberIds: string[]; action: 'fetch' | 'pull'; branchFilter: string | null }
export interface BatchCandidate { memberId: string; repositoryId: string | null; label: string; eligible: boolean; reason: string | null; destination: string | null }
export interface BatchPlan { planId: string; expiresAt: string; candidates: BatchCandidate[]; action: 'fetch' | 'pull' }
export interface Batch { batchId: string; operationIds: string[] }
export interface LaunchRequest { requestId: string; kind: 'repository' | 'project' | 'welcome' | 'discovery'; path: string | null; error: AppError | null; scanDepth?: number | null }
export interface DiscoveryCandidate { repositoryId: string; displayPath: string; label: string }
export interface DiscoveryIssue { path: string; reason: string }
export interface DiscoveryResult { rootPath: string; depth: number; candidates: DiscoveryCandidate[]; issues: DiscoveryIssue[]; truncated: boolean; visitedDirectories: number }
export interface DiffRequest { repositoryId: string; snapshotId: string; fileId: string; context: 'staged' | 'unstaged' | 'commit' }
export interface RewriteRecord { recoveryId: string; phase: string; branch: string; originalHead: string; replacementHead: string | null; baseOid: string; treeOid: string; backupRef: string; replacementRef: string | null; commitOids: string[]; message: string; createdAt: string; error: string | null; canRecover: boolean }
