import { invoke } from '@tauri-apps/api/core';
import type * as T from './types';

export function errorMessage(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error) return String(error.message);
  return String(error);
}
export const api = {
  openRepository: (path: string) => invoke<T.RepositoryOpen>('open_repository', { path }),
  openProject: (path: string) => invoke<T.Project>('open_project', { path }),
  refreshProject: (projectId: string) => invoke<T.Project>('refresh_project', { projectId }),
  saveProject: (projectId: string, expectedRevision: string, content: T.ProjectContent) => invoke<T.Project>('save_project', { projectId, expectedRevision, content }),
  createProject: (path: string, content: T.ProjectContent) => invoke<T.Project>('create_project', { path, content }),
  getStatus: (repositoryId: string) => invoke<T.RepositoryStatus>('get_status', { repositoryId }),
  getChanges: (repositoryId: string) => invoke<T.Changes>('get_changes', { repositoryId }),
  listRefs: (repositoryId: string) => invoke<T.GitRef[]>('list_refs', { repositoryId }),
  listRemotes: (repositoryId: string) => invoke<T.Remote[]>('list_remotes', { repositoryId }),
  getHistory: (repositoryId: string, scope: string | null = null, snapshotId: string | null = null, cursor: string | null = null) => invoke<T.HistoryPage>('get_history', { repositoryId, scope, snapshotId, cursor }),
  getCommit: (repositoryId: string, oid: string, parentOid: string | null = null) => invoke<T.CommitDetails>('get_commit', { repositoryId, oid, parentOid }),
  getDiff: (request: T.DiffRequest) => invoke<T.DiffResult>('get_diff', { request }),
  planAction: (repositoryId: string, action: T.RepositoryAction) => invoke<T.RepositoryActionPlan>('plan_repository_action', { repositoryId, action }),
  runAction: (planId: string) => invoke<T.OperationResult>('run_repository_action', { planId }),
  planBatch: (request: T.BatchRequest) => invoke<T.BatchPlan>('plan_batch', { request }),
  runBatch: (planId: string) => invoke<T.Batch>('run_batch', { planId }),
  cancelBatch: (batchId: string) => invoke<void>('cancel_batch', { batchId }),
  getOperations: (batchId: string | null = null) => invoke<T.OperationResult[]>('get_operations', { batchId }),
  getLaunchRequest: () => invoke<T.LaunchRequest | null>('get_launch_request'),
  ackLaunchRequest: (requestId: string) => invoke<void>('ack_launch_request', { requestId }),
};
