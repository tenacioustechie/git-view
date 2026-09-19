import wire from './wire.json';
import type { RepositoryStatus, HistoryPage, CommitDetails, RepositoryRow, OperationResult, DiffResult } from '../../api/types';
export const fixtures = wire as {
  clean: RepositoryStatus; mixed: RepositoryStatus; detached: RepositoryStatus;
  unborn: RepositoryStatus; conflict: RepositoryStatus; missingUpstream: RepositoryStatus;
  unknownCounts: RepositoryStatus; missingMember: RepositoryRow; failedAction: OperationResult;
  binaryDiff: DiffResult; history: HistoryPage; historyNext: HistoryPage; mergeCommit: CommitDetails;
};
