import type { PullMode } from '../../api/types';

export const PULL_MODES: PullMode[] = ['rebase', 'merge', 'fastForward'];
export const pullModeLabels: Record<PullMode, string> = { rebase: 'Pull with rebase', merge: 'Pull with merge', fastForward: 'Pull fast-forward only' };
export const pullModeNames: Record<PullMode, string> = { rebase: 'Rebase', merge: 'Merge', fastForward: 'Fast-forward only' };
