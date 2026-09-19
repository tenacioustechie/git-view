export type RepositoryId = string;
export type ProjectId = string;
export type SnapshotId = string;
export type FileId = string;
export type Oid = string;

export type Head =
  | { kind: 'branch'; name: string; oid: Oid }
  | { kind: 'detached'; oid: Oid }
  | { kind: 'unborn'; name: string };

export type GitState = 'idle' | 'merge' | 'rebase' | 'cherryPick' | 'revert' | 'other';

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
  | { kind: 'commit'; statusSnapshotId: SnapshotId; message: string }
  | { kind: 'fetch'; remoteId: string }
  | { kind: 'pull'; statusSnapshotId: SnapshotId }
  | { kind: 'push'; statusSnapshotId: SnapshotId; remoteId: string; destinationRef: string; setUpstream: boolean }
  | { kind: 'createBranch'; statusSnapshotId: SnapshotId; name: string; startOid: Oid; switchAfterCreate: boolean }
  | { kind: 'switchBranch'; statusSnapshotId: SnapshotId; localRef: string }
  | { kind: 'merge' | 'squashMerge'; statusSnapshotId: SnapshotId; sourceRef: string }
  | { kind: 'continueMerge' | 'abortMerge'; statusSnapshotId: SnapshotId };

export type RepositoryActionPlan =
  | { kind: 'eligible'; planId: string; expiresAt: string; summary: string; destination: string | null }
  | { kind: 'blocked'; reason: string; error: AppError | null };

export type DiffResult =
  | { kind: 'text'; text: string; truncated: boolean }
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

export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'skipped' | 'canceled';
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
export interface ProjectContent { version: 1; name: string; repositories: { path: string; label?: string }[] }
export interface Project { projectId: string; revision: string; displayPath: string; name: string; content: ProjectContent; members: RepositoryRow[]; stale: boolean; error: AppError | null }
export interface BatchRequest { projectId: string; revision: string; memberIds: string[]; action: 'fetch' | 'pull'; branchFilter: string | null }
export interface BatchCandidate { memberId: string; repositoryId: string | null; label: string; eligible: boolean; reason: string | null; destination: string | null }
export interface BatchPlan { planId: string; expiresAt: string; candidates: BatchCandidate[]; action: 'fetch' | 'pull' }
export interface Batch { batchId: string; operationIds: string[] }
export interface LaunchRequest { requestId: string; kind: 'repository' | 'project' | 'welcome'; path: string | null; error: AppError | null }
export interface DiffRequest { repositoryId: string; snapshotId: string; fileId: string; context: 'staged' | 'unstaged' | 'commit' }
