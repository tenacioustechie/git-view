// Imported explicitly by the coordinator's native-smoke build only. This drives
// the actual React DOM and Tauri commands: no renderer or Git mocks are used.
import { invoke } from '@tauri-apps/api/core';
import { api, errorMessage } from '../../api/client';
import type { OperationResult } from '../../api/types';
import { VIEW_STORAGE_KEY } from './useViewPreferences';

interface SmokeContext { projectPath: string; cleanPath: string; dirtyPath: string; mergePath: string; m2?: boolean; m3?: boolean; performance?: boolean; discovery?: boolean; historySearch?: boolean; remoteCheckout?: boolean; cherryPick?: boolean; stash?: boolean; hunks?: boolean; rebase?: boolean }
interface SmokeResult { passed: boolean; checks: string[]; error?: string; body?: string }
interface ReloadMarker { version: 1; repositoryId: string; operationId: string; path: string; checks: string[] }
const reloadKey = 'git-view.native-smoke.reload.v1';
const stashReloadKey = 'git-view.native-smoke.stash-reload.v1';
const cherryReloadKey = 'git-view.native-smoke.cherry-reload.v1';
interface CherryFixture { repositoryId: string; path: string; sourceOid: string; targetOid: string; sourceRef: string; conflictPaths: string[] }
interface CherryReloadMarker { fixture: CherryFixture; checks: string[] }
let progressWrite: Promise<unknown> = Promise.resolve();
function progressChecks(): string[] {
  const checks: string[] = [];
  checks.push = (...items: string[]) => {
    const length = Array.prototype.push.apply(checks, items);
    const snapshot = [...checks];
    // Serialize writes so a slow earlier diagnostic cannot overwrite a newer
    // checkpoint. Diagnostic failures must not change the acceptance outcome.
    progressWrite = progressWrite.then(() => invoke('native_smoke_progress', { checks: snapshot })).catch(() => undefined);
    return length;
  };
  progressWrite = progressWrite.then(() => invoke('native_smoke_progress', { checks: [] })).catch(() => undefined);
  return checks;
}
const pause = (ms: number) => new Promise<void>(resolve => window.setTimeout(resolve, ms));
function assert(value: unknown, message: string): asserts value { if (!value) throw new Error(message); }
function visible(element: Element) { return !element.closest('[hidden]') && element.getClientRects().length > 0; }
async function until<T>(description: string, read: () => T | false | null | undefined | Promise<T | false | null | undefined>, timeout = 60000): Promise<T> {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const value = await read();
    if (value !== false && value !== null && value !== undefined) return value;
    await pause(100);
  }
  throw new Error(`Timed out: ${description}. Visible alerts: ${[...document.querySelectorAll('[role="alert"]')].filter(visible).map(node => node.textContent).join(' | ')}`);
}
async function button(name: string | RegExp, scope: ParentNode = document) {
  return until(`enabled button ${String(name)}`, () => [...scope.querySelectorAll<HTMLButtonElement>('button')].find(node => {
    const text = node.getAttribute('aria-label') ?? node.textContent?.trim() ?? '';
    return visible(node) && !node.disabled && (typeof name === 'string' ? text === name : name.test(text));
  }));
}
async function click(name: string | RegExp, scope: ParentNode = document) { (await button(name, scope)).click(); }
function type(element: HTMLTextAreaElement | HTMLInputElement, value: string) {
  const prototype = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(prototype, 'value')!.set!.call(element, value);
  element.dispatchEvent(new Event('input', { bubbles: true }));
  element.dispatchEvent(new Event('change', { bubbles: true }));
}
async function region(label: string) {
  return until(`visible ${label}`, () => [...document.querySelectorAll<HTMLElement>('[aria-label]')].find(node => node.getAttribute('aria-label') === label && visible(node)));
}
async function stageFile(path: string) {
  const row = await until(`stageable row ${path}`, () => [...document.querySelectorAll<HTMLButtonElement>('.changes-workspace .file-list .file-preview')]
    .find(node => node.title === path && visible(node) && [...(node.closest('li')?.querySelectorAll('button') ?? [])].some(action => action.textContent === 'Stage'))?.closest('li'));
  await click('Stage', row);
}
async function operation(action: string, repositoryId: string, trigger: () => Promise<void>) {
  const previous = new Set((await api.getOperations()).map(item => item.operationId));
  await trigger();
  return until<OperationResult>(`${action} completes`, async () => (await api.getOperations()).find(item =>
    item.repositoryId === repositoryId && item.action === action && !previous.has(item.operationId) && !['queued', 'running'].includes(item.state)), 120000);
}
async function refreshRepository() {
  const repository = await region('Repository');
  await click('Refresh', repository);
  await until('repository refresh complete', () => {
    const buttons = [...repository.querySelectorAll<HTMLButtonElement>('button')];
    return buttons.some(node => node.getAttribute('aria-label') === 'Refresh' && !node.disabled);
  });
}
async function assertDiffVisible() {
  const diff = await region('File diff');
  await until('text diff renders', () => diff.querySelector('.unified-diff,.split-diff'));
  assert(diff.querySelector('.unified-diff,.split-diff')!.textContent?.trim(), 'Selected file must render an actual patch');
}

async function chooseDiffLayout(panel: HTMLElement, value: 'split' | 'unified') {
  const select = panel.querySelector<HTMLSelectElement>('[aria-label="Diff layout"]')!;
  assert(select && visible(select), 'Text preview must expose its layout control');
  select.value = value; select.dispatchEvent(new Event('change', { bubbles: true }));
  await until(`${value} layout rendered`, () => panel.querySelector(value === 'split' ? '.split-diff' : '.unified-diff'));
  assert(JSON.parse(localStorage.getItem(VIEW_STORAGE_KEY)!).preferences.diffLayout === value, 'Diff layout must be saved');
}

async function focusCommitEditor(repository: HTMLElement) {
  let diagnostics = '';
  let stableFrames = 0;
  let previous: HTMLTextAreaElement | null = null;
  let lastForegroundRequest = -Infinity;
  try {
    return await until('stable commit editor focus and visible outline', async () => {
      if (!document.hasFocus() && performance.now() - lastForegroundRequest >= 500) {
        lastForegroundRequest = performance.now();
        await invoke('native_smoke_focus');
      }
      const input = repository.querySelector<HTMLTextAreaElement>('#commit-message');
      if (!input?.isConnected) { diagnostics = 'Commit textarea is absent or detached'; return null; }
      if (document.activeElement !== input) input.focus();
      await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
      const style = getComputedStyle(input);
      diagnostics = JSON.stringify({ connected: input.isConnected, documentFocused: document.hasFocus(), active: document.activeElement?.tagName, focused: input.matches(':focus'), focusVisible: input.matches(':focus-visible'), outlineStyle: style.outlineStyle, outlineWidth: style.outlineWidth });
      const ready = input.isConnected && document.activeElement === input && style.outlineStyle !== 'none' && Number.parseFloat(style.outlineWidth) >= 2;
      stableFrames = ready ? (previous === input ? stableFrames + 1 : 1) : 0;
      previous = input;
      return stableFrames >= 2 ? input : null;
    }, 8000);
  } catch (cause) {
    throw new Error(`${errorMessage(cause)} Focus diagnostics: ${diagnostics}`);
  }
}

async function checkHistoryKeyboardAndAlignment(repository: HTMLElement, checks: string[]) {
  const scroll = repository.querySelector<HTMLElement>('.history-list');
  assert(scroll, 'History scroll viewport must exist');
  await until('multiple real history rows rendered', () => scroll.querySelectorAll('.history-row').length > 1);
  const rows = [...scroll.querySelectorAll<HTMLButtonElement>('.history-row')];
  const firstOid = rows[0].title.split('\n')[0];
  rows[0].click();
  await until('initial history selection applied', () => rows[0].getAttribute('aria-pressed') === 'true');
  scroll.focus();
  assert(document.activeElement === scroll, 'History viewport must accept keyboard focus');
  rows[0].focus();
  rows[0].dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
  await until('ArrowDown selects next actual commit', () => scroll.querySelector<HTMLButtonElement>('.history-row[aria-pressed="true"]')?.title.split('\n')[0] === rows[1].title.split('\n')[0]);
  assert(document.activeElement === scroll, 'Arrow navigation from a virtual row must transfer focus to the stable viewport');
  assert(scroll.querySelector('[role="status"]')?.textContent?.includes(rows[1].title.split('\n')[0].slice(0, 8)), 'Selected commit must have a live text announcement');
  scroll.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }));
  await until('Home restores tip selection', () => scroll.querySelector<HTMLButtonElement>('.history-row[aria-pressed="true"]')?.title.split('\n')[0] === firstOid);
  for (const row of rows.slice(0, 5)) {
    const rowBox = row.getBoundingClientRect();
    const svg = row.querySelector('svg')!;
    const svgBox = svg.getBoundingClientRect();
    const circle = row.querySelector('circle')!;
    const nodeCenter = svgBox.top + Number(circle.getAttribute('cy'));
    assert(Math.abs(rowBox.height - 44) <= 1 && Math.abs(svgBox.height - rowBox.height) <= 1, 'SVG edge height must align with virtualized row height');
    assert(Math.abs(nodeCenter - (rowBox.top + rowBox.height / 2)) <= 1, 'Graph node must align to the center of its text row');
  }
  checks.push('Native history keyboard ArrowDown/Home selected actual commits; focused viewport and SVG node/edge geometry align with 44px virtual rows');
  const more = await button('More repository actions', repository);
  more.focus(); more.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
  await until('ArrowUp opens More at its last command', () => document.activeElement?.textContent === 'Combine commits…');
  document.activeElement?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }));
  await until('Home selects Find with shortcut hint', () => document.activeElement?.getAttribute('aria-label') === 'Find in history…');
  assert(document.activeElement?.querySelector('kbd'), 'Find command must display its shortcut');
  document.activeElement?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  assert(document.activeElement === more, 'Escape must restore More trigger focus');
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', ctrlKey: true, bubbles: true, cancelable: true }));
  await until('repository shortcut targets visible history, not hidden project', () => document.activeElement === repository.querySelector('[aria-label="History search query"]'));
  checks.push('Native More menu opens at last command with ArrowUp, Home reaches hinted Find, Escape returns focus; primary-F focuses visible repository history, and virtual-row arrows preserve stable focus with selected-commit announcement');
}

function textContrast(foreground: string, background: string) {
  const luminance = (color: string) => {
    const channels = color.match(/[\d.]+/g)?.slice(0, 3).map(Number);
    assert(channels?.length === 3 && color.startsWith('rgb'), `Expected native computed RGB color, got ${color}`);
    const linear = channels.map(channel => { const value = channel / 255; return value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4; });
    return linear[0] * .2126 + linear[1] * .7152 + linear[2] * .0722;
  };
  const a = luminance(foreground), b = luminance(background);
  return (Math.max(a, b) + .05) / (Math.min(a, b) + .05);
}

async function checkNativeAppearance(checks: string[]) {
  const originalTheme = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  const originalWidth = window.innerWidth, originalHeight = window.innerHeight;
  const colors: string[] = [];
  try {
    for (const theme of ['dark', 'light'] as const) {
      const width = theme === 'dark' ? 1280 : 1000;
      const height = theme === 'dark' ? 800 : 700;
      await invoke('native_smoke_appearance', { theme, width, height });
      await until(`native ${theme} appearance`, () => window.matchMedia('(prefers-color-scheme: dark)').matches === (theme === 'dark'));
      await until(`native viewport width ${width}`, () => Math.abs(window.innerWidth - width) <= 5);
      await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
      const style = getComputedStyle(document.documentElement);
      colors.push(style.backgroundColor);
      assert(textContrast(style.color, style.backgroundColor) >= 4.5, `${theme} body text must have readable contrast against the background`);
      const panels = document.querySelector<HTMLElement>('.repository-panels');
      assert(panels && panels.clientHeight > 100 && panels.clientWidth > 700, `${theme} resized repository panels must remain usable`);
      const viewport = document.querySelector<HTMLElement>('.history-list');
      assert(viewport && viewport.clientHeight > 100 && viewport.getBoundingClientRect().bottom <= window.innerHeight + 1, 'History viewport must stay bounded inside resized window');
    }
    assert(colors[0] !== colors[1], 'Native appearance switch must change the rendered theme palette');
    checks.push('Native dark/light window themes changed rendered palettes with body text contrast ≥4.5; 1280×800 and 1000×700 layouts retained bounded usable history panels');
  } finally {
    await invoke('native_smoke_appearance', { theme: originalTheme, width: originalWidth, height: originalHeight });
  }
}

async function runMergeSmoke(checks: string[]) {
  const scenarios = ['merge', 'squash', 'mergeContinue', 'mergeAbort', 'squashConflict'] as const;
  for (const scenario of scenarios) {
    const fixture = await invoke<{ repositoryId: string; path: string; sourceRef: string }>('native_smoke_m2_case', { scenario });
    const opened = await api.openRepository(fixture.path);
    const repository = await until(`M2 ${scenario} repository opens`, () => {
      const node = document.querySelector<HTMLElement>('.repository-screen');
      return node?.querySelector('.repository-title strong')?.getAttribute('title') === opened.displayPath ? node : null;
    });
    const before = await api.getStatus(fixture.repositoryId);
    assert(before.head.kind === 'branch' && !before.dirty && before.gitState === 'idle', `${scenario} fixture must start clean on an attached branch`);
    const originalHead = before.head.oid;
    await click('History', repository);
    const source = (await api.listRefs(fixture.repositoryId)).find(ref => ref.fullName === fixture.sourceRef);
    assert(source, `${scenario} source branch must exist`);
    const squash = scenario === 'squash' || scenario === 'squashConflict';
    const conflicts = scenario === 'mergeContinue' || scenario === 'mergeAbort' || scenario === 'squashConflict';
    const action = squash ? 'squashMerge' : 'merge';
    const controls = repository.querySelector<HTMLDetailsElement>('.merge-controls');
    assert(controls, 'Merge branch controls must be available');
    if (!controls.open) controls.querySelector('summary')!.click();
    const selector = await until('merge source loaded', () => {
      const node = controls.querySelector<HTMLSelectElement>('[aria-label="Merge source branch"]');
      return node && [...node.options].some(option => option.value === fixture.sourceRef) ? node : null;
    });
    selector.value = fixture.sourceRef;
    selector.dispatchEvent(new Event('change', { bubbles: true }));
    const mergeOperation = await operation(action, fixture.repositoryId, async () => {
      await click(squash ? 'Squash merge branch' : 'Merge branch', controls);
      const preview = await until('merge preview', () => repository.querySelector<HTMLDialogElement>('dialog[open]'));
      assert(preview.textContent?.includes(fixture.sourceRef) && preview.textContent.includes('Current target:'), 'Merge preview must show source and target');
      await click(`Run ${action}`, preview);
    });
    assert(mergeOperation.state === (conflicts ? 'failed' : 'succeeded'), `${scenario} initial merge outcome: ${mergeOperation.state}, ${mergeOperation.error?.message ?? mergeOperation.reason}`);
    await refreshRepository();
    let status = await api.getStatus(fixture.repositoryId);
    if (conflicts) {
      assert(status.counts.conflicted > 0 && status.gitState === (squash ? 'squashMerge' : 'merge'), `${scenario} must retain a persistent conflict state`);
      assert(status.head.kind === 'branch' && status.head.oid === originalHead, 'Conflicted merge must not advance HEAD');
      if (squash) {
        assert(repository.textContent?.includes('does not create MERGE_HEAD'), 'Squash conflicts must explain the absent ordinary merge state');
        assert(![...repository.querySelectorAll('button')].some(node => node.textContent === 'Abort merge'), 'Squash conflict must not offer destructive ordinary abort');
      } else {
        const continuation = [...repository.querySelectorAll<HTMLButtonElement>('button')].find(node => node.textContent === 'Continue merge');
        assert(continuation?.disabled, 'Continue merge must remain disabled until conflicts are staged');
      }
      if (scenario === 'mergeAbort') {
        const aborted = await operation('abortMerge', fixture.repositoryId, async () => {
          await click('Abort merge', repository);
          const preview = await until('abort preview', () => repository.querySelector<HTMLDialogElement>('dialog[open]'));
          await click('Run abortMerge', preview);
        });
        assert(aborted.state === 'succeeded', `Abort failed: ${aborted.error?.message ?? aborted.reason}`);
        status = await api.getStatus(fixture.repositoryId);
        assert(status.gitState === 'idle' && !status.dirty && status.head.kind === 'branch' && status.head.oid === originalHead, 'Abort must restore the original clean branch');
        checks.push('M2 DOM ordinary merge conflict exposed abort; previewed abort restored original clean HEAD');
        continue;
      }
      const unresolved = (await api.getChanges(fixture.repositoryId)).files.filter(file => file.conflicted).map(file => file.displayPath);
      await invoke('native_smoke_resolve_conflicts', { repositoryId: fixture.repositoryId });
      await refreshRepository();
      for (const path of unresolved) {
        const staged = await operation('stage', fixture.repositoryId, async () => {
          const row = await until(`conflict row ${path}`, () => [...repository.querySelectorAll<HTMLButtonElement>('.file-list button[title]')].find(node => node.title === path)?.closest('li'));
          await click('Stage resolution', row);
        });
        assert(staged.state === 'succeeded', 'Conflict resolution stage must succeed');
        await refreshRepository();
      }
      status = await api.getStatus(fixture.repositoryId);
      assert(status.counts.conflicted === 0, 'All conflict resolutions must be staged');
    }
    if (squash) {
      assert(status.gitState === 'squashMerge' && status.head.kind === 'branch' && status.head.oid === originalHead && status.counts.staged > 0, 'Squash must leave staged review without moving HEAD');
      assert(status.squashMerge?.originalHead === originalHead && status.squashMerge.sourceOid === source.oid, 'Persistent squash review must retain source and original HEAD');
      assert(repository.textContent?.includes('Squash merge review'), 'Renderer must show persistent squash review');
      const message = `Native ${scenario} reviewed result`;
      const input = await until('squash message', () => repository.querySelector<HTMLTextAreaElement>('#commit-message'));
      type(input, message);
      const committed = await operation('commit', fixture.repositoryId, () => click(/^Commit staged changes \(/, repository));
      assert(committed.state === 'succeeded', `Squash explicit commit failed: ${committed.error?.message ?? committed.reason}`);
      status = await api.getStatus(fixture.repositoryId);
      assert(status.gitState === 'idle' && status.head.kind === 'branch', 'Explicit squash commit must clear review state');
      const details = await api.getCommit(fixture.repositoryId, status.head.oid);
      assert(details.parents.length === 1 && details.parents[0] === originalHead && details.message.trim() === message, 'Squash result must be one normal commit on original HEAD with reviewed message');
      checks.push(`M2 DOM ${scenario === 'squashConflict' ? 'squash conflict resolved and staged externally' : 'squash merge left staged review'}; explicit normal commit has one parent and chosen message`);
    } else {
      if (scenario === 'mergeContinue') {
        const continued = await operation('continueMerge', fixture.repositoryId, async () => {
          await click('Continue merge', repository);
          const preview = await until('continue merge preview', () => repository.querySelector<HTMLDialogElement>('dialog[open]'));
          await click('Run continueMerge', preview);
        });
        assert(continued.state === 'succeeded', `Continue merge failed: ${continued.error?.message ?? continued.reason}`);
        status = await api.getStatus(fixture.repositoryId);
      }
      assert(status.gitState === 'idle' && status.head.kind === 'branch', 'Successful ordinary merge must end idle on branch');
      const details = await api.getCommit(fixture.repositoryId, status.head.oid);
      assert(details.parents.length === 2 && details.parents[0] === originalHead && details.parents[1] === source.oid, 'Ordinary merge must preserve both actual parents');
      checks.push(`M2 DOM ordinary merge ${scenario === 'mergeContinue' ? 'conflict resolution and Continue merge' : 'preview and execution'} produced a two-parent merge`);
    }
  }
}

async function runCombineSmoke(checks: string[]) {
  for (const scenario of ['combine', 'hookFailure', 'staleRecovery'] as const) {
    const fixture = await invoke<{ repositoryId: string; path: string; commitOids: string[]; baseOid: string; treeOid: string }>('native_smoke_m3_case', { scenario });
    const opened = await api.openRepository(fixture.path);
    const repository = await until(`M3 ${scenario} repository opens`, () => {
      const node = document.querySelector<HTMLElement>('.repository-screen');
      return node?.querySelector('.repository-title strong')?.getAttribute('title') === opened.displayPath ? node : null;
    });
    assert(opened.status.head.kind === 'branch' && !opened.status.dirty, 'Combine fixture must be clean on a branch');
    const originalHead = opened.status.head.oid;
    assert(fixture.commitOids.length === 2 && fixture.commitOids[0] === originalHead, 'Fixture must identify the latest two commits');
    await click('More repository actions', repository);
    await click('Combine commits…', repository);
    const panel = await region('Combine commits');
    const count = await until('combine count', () => panel.querySelector<HTMLInputElement>('[aria-label="Newest commits to combine"]'));
    type(count, '1');
    await until('one-commit selection rejected', () => panel.querySelector('.combine-blocked')?.textContent === 'Choose at least two commits.');
    const currentCount = await until('current combine count', () => panel.querySelector<HTMLInputElement>('[aria-label="Newest commits to combine"]'));
    type(currentCount, '2');
    await until('exact selected suffix rendered', () => fixture.commitOids.every(oid => panel.querySelector('[aria-label="Commits to replace"]')?.textContent?.includes(oid)));
    const message = `Native ${scenario} combined message`;
    const input = await until('combined message editor', () => panel.querySelector<HTMLTextAreaElement>('[aria-label="Combined commit message"]'));
    type(input, message);
    const combined = await operation('combineCommits', fixture.repositoryId, async () => {
      await click('Preview combine commits', panel);
      const preview = await until('combine review modal', () => repository.querySelector<HTMLDialogElement>('dialog[open]'));
      assert(fixture.commitOids.every(oid => preview.textContent?.includes(oid)), 'Authoritative combine preview must list exact selected commits');
      assert(preview.textContent?.includes(message) && preview.textContent.includes('may already be published'), 'Combine preview must include edited message and publication warning');
      await click('Run combineCommits', preview);
    });
    await refreshRepository();
    const record = (await api.listRewrites(fixture.repositoryId)).find(item => item.message === message);
    assert(record, 'Combine operation must leave a durable recovery record');
    assert(record.originalHead === originalHead && record.baseOid === fixture.baseOid && record.treeOid === fixture.treeOid, 'Recovery record must retain original head, replacement parent and final tree');
    const verify = async (expected: 'combined' | 'recovered' | 'stale' | 'hookFailure') => {
      const verified = await invoke<{ verified: boolean }>('native_smoke_m3_verify', { repositoryId: fixture.repositoryId, recoveryId: record.recoveryId, expected });
      assert(verified.verified, `Native fixture verification failed: ${scenario}/${expected}`);
    };
    if (scenario === 'hookFailure') {
      assert(combined.state === 'failed', 'Rejecting hook must fail the combine candidate commit');
      const unchanged = await api.getStatus(fixture.repositoryId);
      assert(unchanged.head.kind === 'branch' && unchanged.head.oid === originalHead && !unchanged.dirty, 'Failed combine must preserve original HEAD/index/worktree');
      const retained = await until('failed combine draft retained', () => panel.querySelector<HTMLTextAreaElement>('[aria-label="Combined commit message"]'));
      assert(retained.value === message, 'Combined message must survive real hook failure');
      await until('failed combine record visible', () => panel.querySelector('.rewrite-record')?.textContent?.includes(record.backupRef));
      await verify('hookFailure');
      checks.push('M3 DOM combine rejected by real hook: original branch/tree unchanged, backup and failed record retained, edited message preserved');
      continue;
    }
    assert(combined.state === 'succeeded', `Combine failed: ${combined.error?.message ?? combined.reason}`);
    const installed = await api.getStatus(fixture.repositoryId);
    assert(installed.head.kind === 'branch' && installed.head.oid === record.replacementHead && installed.head.oid !== originalHead, 'Combined replacement must be installed on original branch');
    const details = await api.getCommit(fixture.repositoryId, installed.head.oid);
    assert(details.parents.length === 1 && details.parents[0] === fixture.baseOid && details.message.trim() === message, 'Replacement must have chosen message and parent before selected suffix');
    await verify('combined');
    await invoke('native_smoke_m3_edit', { repositoryId: fixture.repositoryId, commit: scenario === 'staleRecovery' });
    await refreshRepository();
    const afterEdits = await api.getStatus(fixture.repositoryId);
    const records = await api.listRewrites(fixture.repositoryId);
    const current = records.find(item => item.recoveryId === record.recoveryId);
    assert(current, 'Recovery journal must persist after external edits');
    const recordElement = await until('recovery record renders', () => [...panel.querySelectorAll<HTMLElement>('.rewrite-record')].find(node => node.textContent?.includes(record.backupRef)));
    if (scenario === 'staleRecovery') {
      assert(!current.canRecover, 'Later branch commit must block automatic recovery');
      await until('stale recovery control disabled', () => [...recordElement.querySelectorAll<HTMLButtonElement>('button')].some(node => node.textContent === 'Preview recovery' && node.disabled));
      const rejected = await api.planAction(fixture.repositoryId, { kind: 'recoverCombine', statusSnapshotId: afterEdits.snapshotId, recoveryId: record.recoveryId });
      assert(rejected.kind === 'blocked', 'Backend must also reject recovery after a later branch commit');
      await verify('stale');
      checks.push('M3 DOM backup record disabled recovery after later branch commit; real backend rejected stale recovery and preserved later history');
    } else {
      assert(afterEdits.dirty && afterEdits.counts.staged > 0 && afterEdits.counts.unstaged > 0 && afterEdits.counts.untracked > 0, 'Recovery fixture must contain later staged, unstaged and untracked edits');
      assert(current.canRecover, 'Later working-file edits must still allow guarded recovery');
      const recovered = await operation('recoverCombine', fixture.repositoryId, async () => {
        await click('Preview recovery', recordElement);
        const preview = await until('recovery preview', () => repository.querySelector<HTMLDialogElement>('dialog[open]'));
        assert(preview.textContent?.includes('preserves working-file edits'), 'Recovery preview must explain edit preservation');
        await click('Run recoverCombine', preview);
      });
      assert(recovered.state === 'succeeded', `Recovery failed: ${recovered.error?.message ?? recovered.reason}`);
      const restored = await api.getStatus(fixture.repositoryId);
      assert(restored.head.kind === 'branch' && restored.head.oid === originalHead, 'Recovery must restore original branch tip');
      assert(JSON.stringify(restored.counts) === JSON.stringify(afterEdits.counts), 'Recovery must preserve staged/unstaged/untracked states');
      await verify('recovered');
      checks.push('M3 DOM exact two-tip combine preserved final tree and backup; recovery restored original history while preserving later staged/unstaged/untracked contents');
    }
  }
}

async function runPerformanceSmoke(checks: string[]) {
  const started = performance.now();
  const fixture = await invoke<{ repositoryId: string; path: string }>('native_smoke_performance');
  const opened = await api.openRepository(fixture.path);
  const repository = await until('performance repository opens', () => {
    const node = document.querySelector<HTMLElement>('.repository-screen');
    return node?.querySelector('.repository-title strong')?.getAttribute('title') === opened.displayPath ? node : null;
  });
  await click('History', repository);
  await invoke('native_smoke_focus');
  const scroll = await until('first 200 history rows laid out', () => {
    const node = repository.querySelector<HTMLDivElement>('.history-list');
    // The live selected-commit announcement precedes the virtual row spacer.
    const layout = node?.querySelector<HTMLElement>(':scope > div');
    return node && layout && Number.parseFloat(layout.style.height) >= 200 * 44 && node.querySelector('.history-row') ? node : null;
  });
  const firstRenderMs = performance.now() - started;
  const rowsBefore = scroll.querySelectorAll('.history-row').length;
  assert(rowsBefore > 0 && rowsBefore < 80, `Virtualization must bound first-page DOM rows: ${rowsBefore}`);
  const firstVisibleOid = () => [...scroll.querySelectorAll<HTMLButtonElement>('.history-row')].find(row => {
    const box = row.getBoundingClientRect(), viewport = scroll.getBoundingClientRect();
    return box.bottom > viewport.top && box.top < viewport.bottom;
  })?.title.split('\n')[0];
  const beforeOid = firstVisibleOid();
  assert(beforeOid && scroll.scrollHeight > scroll.clientHeight, 'History viewport must be bounded and scrollable');
  const scrolled = performance.now();
  scroll.scrollTop = scroll.scrollHeight;
  scroll.dispatchEvent(new Event('scroll'));
  await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
  await until('virtual history scroll changes visible OID', () => firstVisibleOid() && firstVisibleOid() !== beforeOid);
  const scrollMs = performance.now() - scrolled;
  const rowsAfter = scroll.querySelectorAll('.history-row').length;
  assert(rowsAfter > 0 && rowsAfter < 80, `Virtualized scrolled DOM rows must stay bounded: ${rowsAfter}`);
  checks.push(`Native WebView 10,000-commit fixture: fixture creation plus navigation to first 200-row layout ${firstRenderMs.toFixed(1)} ms; scroll including two animation frames ${scrollMs.toFixed(1)} ms; rendered rows ${rowsBefore} before/${rowsAfter} after, visible OID changed`);
}

interface DiscoveryFixture { rootPath: string; projectPath: string; candidatePaths: string[]; exists: boolean }
function lexicalDiscoveryRoot(path: string) {
  let normalized = path.replace(/\\/g, '/');
  // CLI cwd.join('.') may retain trailing '/.'. Only remove trailing lexical
  // separators/dot components: never resolve '..', case, or symlink identities.
  while ((normalized.endsWith('/.') || normalized.endsWith('/')) && normalized !== '/' && !/^[a-z]:\/$/i.test(normalized)) {
    normalized = normalized.slice(0, normalized.endsWith('/.') ? -2 : -1);
  }
  return normalized;
}
async function runDiscoverySmoke(checks: string[]) {
  const fixtureAction = (action: 'setup' | 'launch' | 'inspect' | 'add' | 'openProject') => invoke<DiscoveryFixture>('native_smoke_discovery_case', { action });
  const fixture = await fixtureAction('setup');
  assert(fixture.candidatePaths.length === 2 && !fixture.exists, 'Discovery fixture must start with two repositories and no project file');
  await fixtureAction('launch');
  const review = await region('Create project from discovery');
  const labelInput = (scope: ParentNode, label: string) => [...scope.querySelectorAll<HTMLLabelElement>('label')].find(node => node.textContent?.trim() === label)?.querySelector<HTMLInputElement>('input');
  const root = labelInput(review, 'Root folder');
  const depth = labelInput(review, 'Search depth');
  assert(root && lexicalDiscoveryRoot(root.value) === lexicalDiscoveryRoot(fixture.rootPath) && depth?.value === '2', `CLI scan must preserve invoking root and explicit depth: ${JSON.stringify({ actualRoot: root?.value, expectedRoot: fixture.rootPath, actualDepth: depth?.value, expectedDepth: '2' })}`);
  assert(review.querySelectorAll('tbody tr').length === 0, 'CLI discovery must open unsaved review before explicit scan');
  await click('Scan repositories', review);
  await until('two discovered candidates displayed', () => review.querySelectorAll('tbody tr').length === 2);
  const candidates = [...review.querySelectorAll<HTMLTableRowElement>('tbody tr')];
  assert(candidates.every(row => !row.querySelector<HTMLInputElement>('input[type="checkbox"]')?.checked), 'Newly discovered repositories must start unchecked');
  assert(fixture.candidatePaths.every(path => candidates.some(row => row.querySelector('.discovery-path')?.textContent === path)), 'Native scan must find actual ordinary and Unicode nested fixture repositories');
  const chosenRow = candidates.find(row => row.querySelector('.discovery-path')?.textContent === fixture.candidatePaths[0]);
  assert(chosenRow, 'Subset candidate must be visible');
  chosenRow.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
  const projectName = labelInput(review, 'Project name');
  assert(projectName, 'Discovery review must expose a project name field');
  type(projectName, 'Native discovered subset');
  await button('Save selected as project (1)', review);
  assert(!(await fixtureAction('inspect')).exists, 'Scanning and selecting must not create or modify a project file');
  checks.push('Discovery warm CLI resolved root/depth; explicit DOM scan found two real repositories including Unicode nested path, initially unchecked; subset review created no project file');

  // Native OS save-picker interaction is deliberately outside this automation.
  // Use the selected DOM candidate with the real create_project IPC command and
  // a test-owned destination, then reopen through the real CLI.
  const selected = candidates.filter(row => row.querySelector<HTMLInputElement>('input[type="checkbox"]')?.checked).map(row => ({
    path: row.querySelector('.discovery-path')!.textContent!, label: row.cells[1].textContent!,
  }));
  assert(selected.length === 1, 'Only the checked discovery candidate may be saved');
  const created = await api.createProject(fixture.projectPath, { version: 1, name: projectName.value, repositories: selected });
  assert((await fixtureAction('inspect')).exists, 'Explicit real create_project IPC must write the selected project');
  await fixtureAction('openProject');
  const group = await until('saved discovered project opens through CLI', () => {
    const node = [...document.querySelectorAll<HTMLElement>('[aria-label="Repository group"]')].find(visible);
    return node?.querySelector('h1')?.textContent === created.name ? node : null;
  });
  assert(group.querySelectorAll('.group-table-scroll tbody tr').length === 1, 'Reopened project must contain only selected candidate');
  const saved = await api.openProject(fixture.projectPath);
  const selectedRepository = await api.openRepository(selected[0].path);
  assert(saved.members.length === 1 && saved.members[0].kind === 'ready' && saved.members[0].repositoryId === selectedRepository.repositoryId, 'Saved subset must resolve to exactly the checked canonical repository');
  checks.push('Discovery DOM-selected subset saved through real create_project IPC and reopened through CLI with exactly one member; native OS save picker was not automated');

  const expanded = await fixtureAction('add');
  const addedPath = expanded.candidatePaths.find(path => !fixture.candidatePaths.includes(path));
  assert(addedPath && expanded.candidatePaths.length === 3, 'Rescan fixture must add exactly one repository');
  const addedRepository = await api.openRepository(addedPath);
  const scanFromGroup = async () => {
    await click('Discover repositories', group);
    const picker = await region('Repository discovery');
    const input = labelInput(picker, 'Root folder');
    assert(input, 'Existing-group discovery must have a root input');
    type(input, fixture.rootPath);
    await click('Scan repositories', picker);
    await until('rescan candidates displayed', () => picker.querySelectorAll('tbody tr').length === 3);
    const existing = [...picker.querySelectorAll<HTMLTableRowElement>('tbody tr')].find(row => row.querySelector('.discovery-path')?.textContent === selected[0].path);
    assert(existing?.querySelector<HTMLInputElement>('input[type="checkbox"]')?.disabled, 'Already saved member must be excluded from duplicate addition');
    const addition = [...picker.querySelectorAll<HTMLTableRowElement>('tbody tr')].find(row => row.querySelector('.discovery-path')?.textContent === addedPath);
    const checkbox = addition?.querySelector<HTMLInputElement>('input[type="checkbox"]');
    assert(checkbox && !checkbox.checked && !checkbox.disabled, 'New repository must be offered as an unchecked addition');
    return { picker, checkbox };
  };
  let scan = await scanFromGroup();
  await click('Rescan', scan.picker);
  await until('rescan completes', () => !scan.picker.textContent?.includes('Scanning repositories…'));
  const untouched = await api.openProject(fixture.projectPath);
  assert(untouched.revision === saved.revision && untouched.members.length === 1, 'Rescan must not silently change saved membership');
  await click('Cancel discovery', scan.picker);
  assert((await api.openProject(fixture.projectPath)).revision === saved.revision, 'Canceling discovery must leave project unchanged');

  scan = await scanFromGroup();
  scan.checkbox.click();
  await click('Add selected to draft (1)', scan.picker);
  const draft = await region('Edit project');
  assert(draft.querySelectorAll('.group-edit-row').length === 2, 'Applying discovery must append only chosen new candidate to the draft');
  assert((await api.openProject(fixture.projectPath)).revision === saved.revision, 'Applying discovered additions to draft must not save automatically');
  await click('Cancel editing', draft);
  assert((await api.openProject(fixture.projectPath)).members.length === 1, 'Canceling addition draft must preserve saved membership');

  scan = await scanFromGroup();
  scan.checkbox.click();
  await click('Add selected to draft (1)', scan.picker);
  await click('Save project', await region('Edit project'));
  await until('explicitly saved discovery addition displayed', () => group.querySelectorAll('.group-table-scroll tbody tr').length === 2);
  const updated = await api.openProject(fixture.projectPath);
  assert(updated.members.length === 2 && updated.members.some(member => member.kind === 'ready' && member.repositoryId === selectedRepository.repositoryId) && updated.members.some(member => member.kind === 'ready' && member.repositoryId === addedRepository.repositoryId), 'Explicit DOM save must preserve original canonical repository and append only selected addition');
  assert(updated.revision !== saved.revision, 'Explicit discovery-addition save must produce a new project revision');
  checks.push('Existing group discovery excluded duplicates and proposed unchecked addition; rescans/cancel/draft cancel preserved saved membership, explicit DOM Save project appended only chosen repository');
}

async function runGroupStatusSmoke(checks: string[]) {
  await invoke('native_smoke_group_status_case', { release: false });
  const group = await until('progressive project opens', () => {
    const node = [...document.querySelectorAll<HTMLElement>('[aria-label="Repository group"]')].find(visible);
    return node?.querySelector('h1')?.textContent === 'Native progressive status' ? node : null;
  });
  await until('14 rows complete around held first repository', () => group.querySelector('[role="status"]')?.textContent?.includes('14 of 15'));
  const rows = group.querySelectorAll('.group-table-scroll tbody tr');
  assert(rows.length === 15 && rows[0].textContent?.includes('Waiting for status'), 'Slow first member must not hold other rows or reorder them');
  assert(rows[14].querySelector('button')?.textContent === 'Locate', 'Missing last repository must retain its locate/error state');
  assert(group.querySelector<HTMLButtonElement>('.group-primary')?.disabled, 'Incomplete status must block batch previews');
  group.querySelector<HTMLInputElement>('[aria-label="Select repo-0"]')!.click();
  const search = group.querySelector<HTMLInputElement>('input[type="search"]')!;
  type(search, 'repo-1');
  await until('selection hidden by filter', () => group.textContent?.includes('1 selected · 1 hidden by filters'));
  await click('repo-1', group);
  await region('Repository');
  await click('Back to Native progressive status');
  checks.push('Progressive group native fixture delivered 14/15 ordered rows around a gated slow read, retained unavailable errors, hidden selection and repository navigation');
  await click('Stop status scan', group);
  await until('status canceled with partial rows', () => group.querySelector('[role="status"]')?.textContent?.includes('Status scan stopped · 14 of 15'));
  assert(group.textContent?.includes('1 selected · 1 hidden by filters'), 'Cancel must preserve selected members and filters');
  await invoke('native_smoke_group_status_case', { release: true });
  await click('Refresh', group);
  await until('progressive retry completes', () => group.querySelector('[role="status"]')?.textContent?.includes('Status checked · 15 of 15'));
  await button('Fetch selected (1)', group);
  type(search, '');
  await until('all members visible after retry', () => group.querySelectorAll('.group-table-scroll tbody tr').length === 15);
  assert(group.querySelector<HTMLInputElement>('[aria-label="Select repo-0"]')!.checked, 'Original member selection must survive cancellation and refresh');
  checks.push('Native Stop status scan retained completed rows; explicit Refresh completed a new scan without losing filters or selection and reenabled batch review');
}

async function runGroupSwitchSmoke(checks: string[]) {
  await invoke('native_smoke_group_switch_case', { inspect: false });
  const group = await until('group-switch project opens', () => {
    const node = [...document.querySelectorAll<HTMLElement>('[aria-label="Repository group"]')].find(visible);
    return node?.querySelector('h1')?.textContent === 'Native group switch' ? node : null;
  });
  group.querySelector<HTMLElement>('.group-switch summary')!.click();
  const target = group.querySelector<HTMLInputElement>('.group-switch input')!;
  type(target, 'team/topic');
  await click('Preview switch all (5)', group);
  const preview = await region('Batch preview');
  assert(preview.querySelectorAll('li').length === 5, 'Switch preview must retain every candidate and skip reason');
  assert([...preview.querySelectorAll('li')].filter(item => item.textContent?.includes('— Eligible')).length === 1, 'Only clean member on another branch may be eligible');
  const inspect = () => invoke<{ branches: string[]; dirtyContents: string }>('native_smoke_group_switch_case', { inspect: true });
  assert((await inspect()).branches.join(',') === 'main,main,main,team/topic', 'Review must not switch any repository');
  type(target, 'different-target');
  checks.push('Group branch switch native preview retained five members with dirty/missing-target/already/unavailable skips and performed no checkout before confirmation');
  await click('Switch 1 eligible to team/topic', preview);
  await until('group-switch results complete', async () => {
    const results = (await api.getOperations()).filter(result => result.batchId && result.action === 'switchBranch');
    return results.length === 5 && results.every(result => !['queued', 'running'].includes(result.state)) ? results : null;
  });
  const results = (await api.getOperations()).filter(result => result.batchId && result.action === 'switchBranch');
  assert(results.filter(result => result.state === 'succeeded').length === 1 && results.filter(result => result.state === 'skipped').length === 4, 'Switch batch must retain one success and four independent skips');
  assert(results.every(result => result.targetBranch === 'team/topic'), 'Batch results must retain original target for retries');
  const actual = await inspect();
  assert(actual.branches.join(',') === 'team/topic,main,main,team/topic' && actual.dirtyContents === 'preserve my edits\n', 'Only eligible repository may switch; dirty bytes must remain unchanged');
  const activity = await region('Batch activity');
  await until('native group-switch activity rendered', () => activity.textContent?.includes('Switch branch batch → team/topic') && activity.textContent.includes('succeeded'));
  checks.push('Native DOM confirmation used the frozen target despite edited input; real Git verified one switch, four skips, retained retry target and preserved dirty bytes');
}

async function runHistorySearchSmoke(checks: string[]) {
  const fixture = await invoke<{ repositoryId: string; path: string; message: string; author: string; body: string; matchOids: string[]; bodyOid: string; otherOid: string; scopeRef: string }>('native_smoke_history_search_case');
  const repository = await until('history search fixture selected', () => {
    const screen = document.querySelector<HTMLElement>('.repository-screen');
    return screen?.querySelector('.repository-title strong')?.getAttribute('title') === fixture.path ? screen : null;
  });
  await click('History', repository);
  const scope = await until('main branch scope available', () => [...repository.querySelectorAll<HTMLButtonElement>('.repository-refs button')].find(node => node.title === fixture.scopeRef));
  scope.click();
  await until('main branch scope selected', () => scope.classList.contains('selected'));
  const query = repository.querySelector<HTMLInputElement>('[aria-label="History search query"]');
  const field = repository.querySelector<HTMLSelectElement>('[aria-label="History search field"]');
  assert(query && field, 'Repository must expose labeled history search controls');
  const search = async (kind: 'message' | 'author' | 'oid', text: string, expected: string[]) => {
    field.value = kind;
    field.dispatchEvent(new Event('change', { bubbles: true }));
    type(query, text);
    await click('Search history', repository);
    const results = await until(`completed ${kind} search for ${text}`, () => {
      const status = repository.querySelector('.history-search [role="status"]')?.textContent ?? '';
      const list = repository.querySelector<HTMLElement>('[aria-label="Search results"]');
      if (!list || !status.includes(text) || status.includes('Searching')) return null;
      const oids = [...list.querySelectorAll<HTMLButtonElement>('.history-row')].map(row => row.title.split('\n')[0]);
      return JSON.stringify(oids) === JSON.stringify(expected) ? list : null;
    });
    assert(!results.querySelector('svg'), 'Filtered history must omit graph edges and nodes');
    return results;
  };
  const results = await search('message', fixture.message, fixture.matchOids);
  assert(![...results.querySelectorAll<HTMLButtonElement>('.history-row')].some(row => row.title.startsWith(fixture.otherOid)), 'Scoped literal search must exclude matching side-branch commit');
  results.querySelector<HTMLButtonElement>('.history-row')!.click();
  const detail = await region('Commit details');
  await until('selected search result actual commit details', () => detail.textContent?.includes(fixture.matchOids[0]));
  const file = await until('selected search commit file', () => detail.querySelector<HTMLButtonElement>('.file-list button'));
  file.click();
  await assertDiffVisible();
  checks.push('Native scoped literal message search returned exact nonadjacent commit OIDs, excluded matching side branch, omitted DAG SVG, and opened actual commit details and file patch');

  const historyTrigger = await button('File history', await region('File diff'));
  historyTrigger.focus(); historyTrigger.click();
  const fileHistory = await until('file history dialog', () => repository.querySelector<HTMLDialogElement>('dialog.file-history-dialog[open]'));
  const fileRows = await until('four ancestor path commits', () => {
    const rows = [...fileHistory.querySelectorAll<HTMLButtonElement>('.history-row')];
    return rows.length === 4 ? rows : null;
  });
  assert(!fileRows.some(row => row.title.startsWith(fixture.otherOid)), 'File history must exclude the disconnected side branch');
  assert(!fileHistory.querySelector('svg'), 'File history must not infer DAG edges');
  fileRows.find(row => row.title.startsWith(fixture.bodyOid))!.click();
  await until('file-history actual older patch', () => fileHistory.querySelector('.unified-diff,.split-diff')?.textContent?.includes('+older match'));
  assert(![...fileHistory.querySelectorAll('button')].some(node => /^(Unstage|Stage) hunk$/.test(node.textContent ?? '')), 'File history remains inspection-only');
  await click('Close file history', fileHistory);
  await until('file history closes with focus return', () => !repository.querySelector('dialog.file-history-dialog') && document.activeElement === historyTrigger);
  checks.push('Native file history opens from an exact historical file, excludes side-branch commits, loads real older patch without mutation controls or false graph edges, and closes with focus return');

  const blameTrigger = await button('Blame', await region('File diff'));
  blameTrigger.focus(); blameTrigger.click();
  const blame = await until('blame dialog', () => repository.querySelector<HTMLDialogElement>('dialog.blame-dialog[open]'));
  const attribution = await until('blame attribution', () => blame.querySelector<HTMLButtonElement>('.blame-lines button'));
  assert(blame.querySelector('.file-history-context')?.textContent?.includes(fixture.matchOids[0]), 'Blame must identify the exact selected revision');
  attribution.click();
  await until('blame actual commit patch', () => blame.querySelector('.unified-diff,.split-diff'));
  assert(![...blame.querySelectorAll('button')].some(node => /^(Unstage|Stage) hunk$/.test(node.textContent ?? '')), 'Blame commit inspection must remain read-only');
  await click('Back to lines', blame);
  assert(document.activeElement?.getAttribute('aria-label') === 'Blame lines', 'Back to lines must return focus');
  const blameVersion = blame.querySelector<HTMLSelectElement>('[aria-label="Blame version"]')!;
  blameVersion.value = 'previous'; blameVersion.dispatchEvent(new Event('change', { bubbles: true }));
  await until('blame comparison-parent content', () => blame.querySelector('.blame-lines pre')?.textContent === 'unmatched middle');
  await click('Close blame', blame);
  await until('blame closes with focus return', () => !repository.querySelector('dialog.blame-dialog') && document.activeElement === blameTrigger);
  checks.push('Native blame reads the selected committed file, opens actual attribution commit patches without mutations, switches to comparison-parent content and closes with keyboard focus return');

  await search('author', fixture.author, fixture.matchOids);
  await search('message', fixture.body, [fixture.bodyOid]);
  await search('oid', fixture.matchOids[0].slice(0, 8), [fixture.matchOids[0]]);
  const empty = await search('message', 'native-no-matching-history-query', []);
  assert(empty.textContent?.includes('No commits match this search.'), 'Successful empty search must be explicit');
  await click('Clear search', repository);
  await until('clear search restores ordinary DAG', () => repository.querySelector('[aria-label="Commit history"] .history-row svg'));
  assert(query.value === '' && !repository.querySelector('[aria-label="Search results"]'), 'Clear must reset query and filtered history');
  checks.push('Native author, full-message body and abbreviated OID searches returned exact commits; no-match state was explicit and Clear restored ordinary history DAG');
}

async function runRemoteCheckoutSmoke(checks: string[]) {
  for (const scenario of ['success', 'dirty', 'collision'] as const) {
    const fixture = await invoke<{ repositoryId: string; path: string; remoteId: string; sourceRef: string; sourceOid: string; upstreamRef: string; localName: string }>('native_smoke_remote_checkout_case', { scenario });
    const repository = await until(`remote checkout ${scenario} fixture selected`, () => {
      const screen = document.querySelector<HTMLElement>('.repository-screen');
      return screen?.querySelector('.repository-title strong')?.getAttribute('title') === fixture.path ? screen : null;
    });
    const initial = await api.getStatus(fixture.repositoryId);
    await click('History', repository);
    const scope = await until('remote history scope available', () => [...repository.querySelectorAll<HTMLButtonElement>('.repository-refs button')].find(node => node.title === fixture.sourceRef));
    scope.click();
    await until('remote scope selected', () => scope.classList.contains('selected'));
    assert(JSON.stringify((await api.getStatus(fixture.repositoryId)).head) === JSON.stringify(initial.head), 'Selecting remote history scope must leave actual HEAD unchanged');
    const summary = await until('remote checkout summary', () => [...repository.querySelectorAll<HTMLElement>('summary')].find(node => node.textContent === 'Check out remote branch'));
    summary.click();
    const panel = await region('Remote branch checkout');
    const select = await until('remote checkout candidate loaded', () => {
      const element = panel.querySelector<HTMLSelectElement>('[aria-label="Remote branch"]');
      return element && !element.disabled && [...element.options].some(option => option.value === JSON.stringify([fixture.remoteId, fixture.sourceRef])) ? element : null;
    });
    select.value = JSON.stringify([fixture.remoteId, fixture.sourceRef]);
    select.dispatchEvent(new Event('change', { bubbles: true }));
    const name = panel.querySelector<HTMLInputElement>('[aria-label="Local branch name"]');
    assert(name, 'Checkout must expose an editable local name');
    type(name, fixture.localName);
    await until('checkout draft name entered', () => name.value === fixture.localName && panel.textContent?.includes(fixture.sourceOid));
    if (scenario === 'success') {
      await click('Preview checkout', panel);
      const preview = await until('remote checkout preview visible', () => repository.querySelector<HTMLDialogElement>('dialog[open]'));
      assert(preview.textContent?.includes(fixture.sourceRef) && preview.textContent.includes(fixture.sourceOid) && preview.textContent.includes(fixture.upstreamRef), 'Checkout preview must show exact source ref, commit and upstream');
      await click('Cancel', preview);
      await until('canceled checkout preview closes', () => !preview.open);
      assert(JSON.stringify((await api.getStatus(fixture.repositoryId)).head) === JSON.stringify(initial.head), 'Canceling checkout preview must not change HEAD');
      assert(name.value === fixture.localName, 'Canceling preview must retain custom local branch name');
      await click('Preview checkout', panel);
      const confirmedPreview = await until('second remote checkout preview visible', () => repository.querySelector<HTMLDialogElement>('dialog[open]'));
      await button('Create and switch', confirmedPreview);
      const result = await operation('checkoutRemoteBranch', fixture.repositoryId, () => click('Create and switch', confirmedPreview));
      assert(result.state === 'succeeded', `Remote tracking checkout failed: ${result.error?.message ?? result.reason}`);
      const actual = await api.getStatus(fixture.repositoryId);
      assert(actual.head.kind === 'branch' && actual.head.name === fixture.localName && actual.head.oid === fixture.sourceOid && actual.upstream, 'Checkout must create the requested branch at the exact source commit with an actual upstream');
      await until('checkout actual branch and upstream refreshed', () => {
        const text = repository.querySelector('.repository-title small')?.textContent ?? '';
        const branch = repository.querySelector<HTMLButtonElement>('button[aria-label="Branch actions"]');
        return branch?.textContent?.trim() === fixture.localName && text.includes(actual.upstream!.ref);
      });
    } else {
      await until('blocked checkout reason', () => panel.textContent?.includes(scenario === 'dirty' ? 'clean index and working tree' : 'already exists at'));
      const preview = [...panel.querySelectorAll<HTMLButtonElement>('button')].find(node => node.textContent === 'Preview checkout');
      assert(preview?.disabled, `${scenario} checkout must remain disabled`);
      if (scenario === 'collision') {
        await click('Review existing local branch', panel);
        await until('separate local-switch review selects existing branch', () => [...repository.querySelectorAll<HTMLSelectElement>('select')].some(node =>
          node.closest('label')?.textContent?.startsWith('Local branch') && !node.hasAttribute('aria-label') && node.value === `refs/heads/${fixture.localName}` && visible(node)));
      }
    }
    const verified = await invoke<{ verified: boolean }>('native_smoke_remote_checkout_verify', { repositoryId: fixture.repositoryId, scenario, localName: fixture.localName });
    assert(verified.verified, `Native remote checkout ${scenario} Git assertions must pass`);
    checks.push(scenario === 'success'
      ? 'Native remote checkout: read-only remote scope, explicit remote/source/upstream preview, custom-name cancel retention, Create and switch, and actual HEAD/upstream/content verification'
      : `Native remote checkout ${scenario}: visible block reason prevented creation; ${scenario === 'collision' ? 'separate existing-local switch review; ' : ''}actual refs/config/working content preserved`);
  }
}

async function cherryRepository(fixture: CherryFixture) {
  return until('cherry-pick fixture selected', () => {
    const repository = document.querySelector<HTMLElement>('.repository-screen');
    return repository?.querySelector('.repository-title strong')?.getAttribute('title') === fixture.path ? repository : null;
  });
}
async function cherryCommand(repository: HTMLElement, repositoryId: string, action: string, label: string, confirm: string) {
  return operation(action, repositoryId, async () => {
    await click(label, repository);
    const preview = await until(`${label} preview visible`, () => repository.querySelector<HTMLDialogElement>('dialog[open]'));
    await click(confirm, preview);
  });
}
async function runCherryPickSmoke(checks: string[]) {
  for (const scenario of ['success', 'conflictAbort', 'empty', 'conflictContinue'] as const) {
    const fixture = await invoke<CherryFixture>('native_smoke_cherry_pick_case', { scenario });
    const repository = await cherryRepository(fixture);
    await click('History', repository);
    const scope = await until('cherry-pick source scope', () => [...repository.querySelectorAll<HTMLButtonElement>('.repository-refs button')].find(node => node.title === fixture.sourceRef));
    scope.click();
    await until('cherry-pick source scope selected', () => scope.classList.contains('selected'));
    const source = await until('cherry-pick source history row', () => [...repository.querySelectorAll<HTMLButtonElement>('.history-row')].find(node => node.title.split('\n')[0] === fixture.sourceOid));
    source.click();
    const before = await api.getStatus(fixture.repositoryId);
    assert(before.head.kind === 'branch' && before.head.oid === fixture.targetOid && before.gitState === 'idle', 'Inspecting the cherry-pick source must leave actual target unchanged');
    const picked = await operation('cherryPick', fixture.repositoryId, async () => {
      await click('Cherry-pick commit', repository);
      const preview = await until('cherry-pick source and target preview', () => repository.querySelector<HTMLDialogElement>('dialog[open]'));
      assert(preview.textContent?.includes(fixture.sourceOid) && preview.textContent.includes(fixture.targetOid), 'Cherry-pick preview must identify exact source and current target');
      await click('Run cherry-pick', preview);
    });
    assert(picked.state === (scenario === 'success' ? 'succeeded' : 'failed'), `${scenario} cherry-pick outcome: ${picked.state}, ${picked.error?.message ?? picked.reason}`);
    await refreshRepository();
    if (scenario === 'success') {
      const verified = await invoke<{ verified: boolean }>('native_smoke_cherry_pick_verify', { repositoryId: fixture.repositoryId, scenario });
      assert(verified.verified, 'Cherry-pick success must preserve source author/message and expected patch/new parent');
      checks.push('Native cherry-pick selected a source in a read-only scope, previewed exact source and actual target, and created the expected patch with source message/author and target parent');
      continue;
    }
    const notice = await region('Git operation');
    assert(notice.textContent?.includes('Cherry-pick in progress') && notice.textContent.includes(fixture.sourceOid), 'Stopped cherry-pick must show operation-specific source context');
    const continuation = [...notice.querySelectorAll<HTMLButtonElement>('button')].find(node => node.textContent === 'Continue cherry-pick');
    assert(continuation?.disabled, 'Continue must stay disabled for unresolved or empty cherry-picks');
    if (scenario === 'empty') {
      assert(notice.textContent?.includes('no remaining changes'), 'Empty cherry-pick must explain why no commit was created');
      const skipped = await cherryCommand(repository, fixture.repositoryId, 'skipCherryPick', 'Skip empty cherry-pick', 'Run skip empty cherry-pick');
      assert(skipped.state === 'succeeded', `Skip empty cherry-pick failed: ${skipped.error?.message ?? skipped.reason}`);
      const verified = await invoke<{ verified: boolean }>('native_smoke_cherry_pick_verify', { repositoryId: fixture.repositoryId, scenario });
      assert(verified.verified, 'Empty skip must finish operation without moving target or creating commit');
      checks.push('Native empty cherry-pick explained the stopped state, disabled Continue, and explicitly skipped without creating a commit or changing target content');
    } else {
      assert(fixture.conflictPaths.every(path => notice.textContent?.includes(path)), 'Conflict guidance must list actual unresolved Git-index files');
      if (scenario === 'conflictAbort') {
        const aborted = await cherryCommand(repository, fixture.repositoryId, 'abortCherryPick', 'Abort cherry-pick', 'Run abort cherry-pick');
        assert(aborted.state === 'succeeded', `Abort cherry-pick failed: ${aborted.error?.message ?? aborted.reason}`);
        const verified = await invoke<{ verified: boolean }>('native_smoke_cherry_pick_verify', { repositoryId: fixture.repositoryId, scenario });
        assert(verified.verified, 'Abort must restore original target and remove operation markers');
        checks.push('Native modify/delete cherry-pick conflict showed files/source/target and disabled Continue; reviewed Abort restored original target and file/index state');
      } else {
        const marker: CherryReloadMarker = { fixture, checks: [...checks] };
        sessionStorage.setItem(cherryReloadKey, JSON.stringify(marker));
        assert(sessionStorage.getItem(cherryReloadKey), 'Cherry-pick reload checkpoint must be retained');
        await progressWrite;
        window.location.reload();
        return;
      }
    }
  }
}
async function finishCherryPickReload(marker: CherryReloadMarker, checks: string[]) {
  const fixture = await invoke<CherryFixture>('native_smoke_cherry_pick_case', { scenario: 'conflictContinue' });
  assert(fixture.repositoryId === marker.fixture.repositoryId && fixture.sourceOid === marker.fixture.sourceOid && fixture.targetOid === marker.fixture.targetOid, 'Reload must reconnect to the original stopped cherry-pick');
  const repository = await cherryRepository(fixture);
  const notice = await region('Git operation');
  await until('reloaded cherry-pick context reconciled', () => notice.textContent?.includes('Cherry-pick in progress') && notice.textContent.includes(fixture.sourceOid) && fixture.conflictPaths.every(path => notice.textContent?.includes(path)));
  const stopped = await api.getStatus(fixture.repositoryId);
  assert(stopped.gitState === 'cherryPick' && stopped.counts.conflicted > 0 && stopped.operationContext?.stopReason === 'conflicts', 'Actual unresolved operation must survive full WebView reload');
  await invoke('native_smoke_resolve_conflicts', { repositoryId: fixture.repositoryId });
  await refreshRepository();
  const continuation = [...repository.querySelectorAll<HTMLButtonElement>('button')].find(node => node.textContent === 'Continue cherry-pick');
  assert(continuation?.disabled && (await api.getStatus(fixture.repositoryId)).counts.conflicted > 0, 'External file edits alone must not enable Continue before Git index staging');
  for (const path of fixture.conflictPaths) {
    const staged = await operation('stage', fixture.repositoryId, async () => {
      const row = await until(`cherry-pick conflict file ${path}`, () => [...repository.querySelectorAll<HTMLButtonElement>('.changes-workspace .file-preview')].find(node => node.title === path)?.closest('li'));
      await click('Stage resolution', row);
    });
    assert(staged.state === 'succeeded', `Cherry-pick resolution stage failed: ${staged.error?.message ?? staged.reason}`);
    await refreshRepository();
  }
  const draft = await until('normal commit draft after resolution', () => repository.querySelector<HTMLTextAreaElement>('#commit-message'));
  type(draft, 'Keep normal draft separate from cherry-pick');
  await until('separate normal draft rendered', () => draft.value === 'Keep normal draft separate from cherry-pick');
  const normalCommit = [...repository.querySelectorAll<HTMLButtonElement>('button')].find(node => node.textContent?.startsWith('Commit staged changes'));
  assert(normalCommit?.disabled, 'Stopped cherry-pick must use Continue rather than a normal commit');
  const continued = await cherryCommand(repository, fixture.repositoryId, 'continueCherryPick', 'Continue cherry-pick', 'Run continue cherry-pick');
  assert(continued.state === 'succeeded', `Continue cherry-pick failed: ${continued.error?.message ?? continued.reason}`);
  const verified = await invoke<{ verified: boolean }>('native_smoke_cherry_pick_verify', { repositoryId: fixture.repositoryId, scenario: 'conflictContinue' });
  assert(verified.verified, 'Continued cherry-pick must preserve source metadata and expected resolved patch');
  assert(draft.value === 'Keep normal draft separate from cherry-pick', 'Continue must retain separate normal commit draft');
  checks.push('Native add/add cherry-pick conflict survived full WebView reload with accurate source/target/files; external edits remained unresolved until DOM Stage resolution, then Continue preserved source metadata and separate normal draft');
}

interface StashFixture { repositoryId: string; path: string; trackedPath: string; untrackedPath: string; stashOid?: string; selectedStashOid?: string }
async function stashRepository(path: string) {
  const repository = await until('stash fixture selected', () => {
    const node = document.querySelector<HTMLElement>('.repository-screen');
    return node?.querySelector('.repository-title strong')?.getAttribute('title') === path ? node : null;
  });
  await button('Refresh', repository);
  return repository;
}
async function selectNativeStash(panel: HTMLElement, oid: string) {
  await until('stash list refresh settled', () => !panel.textContent?.includes('Loading stashes…'));
  const row = await until(`stash ${oid} listed`, () => [...panel.querySelectorAll<HTMLButtonElement>('.stash-list button')].find(node => node.title === oid));
  row.click();
  return until('selected stash metadata', () => {
    const selected = panel.querySelector<HTMLElement>('[aria-label="Selected stash"]');
    return selected?.querySelector('code')?.textContent === oid ? selected : null;
  });
}
async function runStashSmoke(checks: string[]) {
  for (const scenario of ['default', 'includeUntracked', 'delete', 'conflict'] as const) {
    const fixture = await invoke<StashFixture>('native_smoke_stash_case', { scenario });
    const repository = await stashRepository(fixture.path);
    await click('Stashes', repository);
    const panel = await region('Stashes');
    if (scenario === 'default' || scenario === 'includeUntracked') {
      const inclusion = await until('untracked stash checkbox', () => [...panel.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')].find(node => node.closest('label')?.textContent?.includes('Include untracked files')));
      assert(!inclusion.checked, 'Untracked inclusion must initially be unchecked');
      const message = panel.querySelector<HTMLInputElement>('[aria-label="Stash message"]');
      assert(message, 'Stash message input must be available');
      const savedMessage = `Native ${scenario} stash`;
      type(message, savedMessage);
      if (scenario === 'includeUntracked') inclusion.click();
      const created = await cherryCommand(repository, fixture.repositoryId, 'createStash', 'Create stash', 'Run create stash');
      assert(created.state === 'succeeded', `Create stash failed: ${created.error?.message ?? created.reason}`);
      // Backend completion precedes the renderer's operation poll. Wait until
      // that poll has observed success before starting multi-section inspection.
      await until('renderer reconciles completed stash creation', () => [...repository.querySelectorAll('.repository-activity li')].some(node => node.querySelector('strong')?.textContent === 'create stash' && node.querySelector('.activity-state')?.textContent === 'succeeded'));
      await button('Refresh', repository);
      const verified = await invoke<{ verified: boolean }>('native_smoke_stash_verify', { repositoryId: fixture.repositoryId, scenario, stage: 'created' });
      assert(verified.verified, 'Created stash must preserve exact saved contents and inclusion behavior');
      await refreshRepository();
      const entry = (await api.listStashes(fixture.repositoryId)).entries.find(entry => entry.message.includes(savedMessage));
      assert(entry, 'Newly created stash must be listed');
      if (scenario === 'default') {
        checks.push('Native Create stash left Include untracked files unchecked; actual saved index/worktree contents verified, tracked worktree cleaned and untracked file retained');
        continue;
      }
      const selected = await selectNativeStash(panel, entry.oid);
      const selector = selected.querySelector<HTMLSelectElement>('[aria-label="Stash section"]');
      assert(selector, 'Saved stash section selector must exist');
      for (const [section, path, contents] of [['worktree', fixture.trackedPath, 'saved working bytes'], ['index', fixture.trackedPath, 'saved index bytes'], ['untracked', fixture.untrackedPath, 'saved untracked bytes']]) {
        selector.value = section; selector.dispatchEvent(new Event('change', { bubbles: true }));
        const file = await until(`actual saved ${section} file ${path}`, () => [...panel.querySelectorAll<HTMLButtonElement>(`[aria-label="Selected stash"] [data-loaded-section="${section}"] button[title]`)].find(node => node.title === path));
        file.click();
        await until(`saved ${section} patch`, () => panel.querySelector('[aria-label="Selected stash"] .unified-diff')?.textContent?.includes(contents));
      }
      const restore = selected.querySelector<HTMLInputElement>('input[type="checkbox"]');
      assert(restore && !restore.checked, 'Restore staged state must initially be unchecked');
      restore.click();
      const applied = await cherryCommand(repository, fixture.repositoryId, 'applyStash', 'Apply stash', 'Run apply stash');
      assert(applied.state === 'succeeded', `Apply stash failed: ${applied.error?.message ?? applied.reason}`);
      const appliedVerification = await invoke<{ verified: boolean }>('native_smoke_stash_verify', { repositoryId: fixture.repositoryId, scenario, stage: 'applied' });
      assert(appliedVerification.verified, 'Apply must restore exact staged/unstaged/untracked bytes and retain the stash');
      await refreshRepository();
      await until('applied stash still listed', () => [...panel.querySelectorAll<HTMLButtonElement>('.stash-list button')].some(node => node.title === entry.oid));
      checks.push('Native opt-in stash saved untracked files; DOM inspected tracked/index/untracked patches; explicit Restore staged state restored exact index/worktree/untracked contents and Apply retained the saved entry');
    } else if (scenario === 'delete') {
      assert(fixture.selectedStashOid, 'Deletion fixture must identify a non-top entry');
      await selectNativeStash(panel, fixture.selectedStashOid);
      await click('Delete stash', panel);
      const preview = await until('non-top stash deletion preview', () => repository.querySelector<HTMLDialogElement>('dialog[open]'));
      assert(preview.textContent?.includes(fixture.selectedStashOid), 'Delete preview must identify the selected exact stash OID');
      const before = (await api.listStashes(fixture.repositoryId)).entries.map(entry => entry.oid);
      await click('Cancel', preview);
      await until('canceled stash preview closes', () => !preview.open);
      assert(JSON.stringify((await api.listStashes(fixture.repositoryId)).entries.map(entry => entry.oid)) === JSON.stringify(before), 'Cancel must preserve every stash entry');
      await button('Refresh', repository);
      await selectNativeStash(panel, fixture.selectedStashOid);
      const deleted = await cherryCommand(repository, fixture.repositoryId, 'deleteStash', 'Delete stash', 'Run delete stash');
      assert(deleted.state === 'succeeded', `Delete stash failed: ${deleted.error?.message ?? deleted.reason}`);
      const verified = await invoke<{ verified: boolean }>('native_smoke_stash_verify', { repositoryId: fixture.repositoryId, scenario, stage: 'deleted' });
      assert(verified.verified, 'Non-top Delete must preserve neighboring entries and working/index contents with retained backup');
      checks.push('Native non-top stash Delete preview identified exact entry; Cancel preserved all entries, reviewed Delete removed only the middle entry, retained backup and preserved working/index contents');
    } else {
      assert(fixture.stashOid, 'Conflict fixture must identify its stash');
      await selectNativeStash(panel, fixture.stashOid);
      const applied = await cherryCommand(repository, fixture.repositoryId, 'applyStash', 'Apply stash', 'Run apply stash');
      assert(applied.state === 'failed', 'Conflicting Apply must report failure and retain saved stash');
      await refreshRepository();
      const notice = await region('Git operation');
      assert(notice.textContent?.includes('Unresolved index conflicts') && notice.textContent.includes(fixture.trackedPath), 'Stash conflict guidance must describe actual index conflicts with unknown source');
      assert(![...notice.querySelectorAll('button')].some(node => /Continue|Abort/.test(node.textContent ?? '')), 'Stash conflicts must not offer ordinary merge or cherry-pick continuation/abort');
      const verified = await invoke<{ verified: boolean }>('native_smoke_stash_verify', { repositoryId: fixture.repositoryId, scenario, stage: 'conflicted' });
      assert(verified.verified, 'Failed stash Apply must retain exact stash with actual Idle conflicted index state');
      checks.push('Native conflicting stash Apply retained saved entry and actual conflicted index; renderer showed generic external-resolution guidance without false merge/cherry-pick Continue or Abort');
    }
  }
  const fixture = await invoke<{ repositoryId: string; path: string; operationId: string }>('native_smoke_slow_stash');
  await until('real stash creation running before reload', async () => {
    const operation = (await api.getOperations()).find(operation => operation.operationId === fixture.operationId);
    assert(!operation || operation.state === 'queued' || operation.state === 'running', 'Slow stash must remain in flight before reload');
    return operation?.state === 'running';
  });
  const marker: ReloadMarker = { version: 1, ...fixture, checks: [...checks] };
  sessionStorage.setItem(stashReloadKey, JSON.stringify(marker));
  assert(sessionStorage.getItem(stashReloadKey), 'Stash reload checkpoint must persist');
  await progressWrite;
  window.location.reload();
}
async function finishStashReload(marker: ReloadMarker, checks: string[]) {
  const fixture = await invoke<{ repositoryId: string; path: string; operationId: string }>('native_smoke_slow_stash');
  assert(fixture.repositoryId === marker.repositoryId && fixture.operationId === marker.operationId && fixture.path === marker.path, 'Stash reload must reconnect to the original operation');
  const completed = await until('original stash creation completes after reload', async () => (await api.getOperations()).find(operation => operation.operationId === fixture.operationId && !['queued', 'running'].includes(operation.state)), 120000);
  assert(completed.state === 'succeeded', `Reloaded stash creation failed: ${completed.error ? JSON.stringify(completed.error) : completed.reason}`);
  const verified = await invoke<{ verified: boolean }>('native_smoke_stash_verify', { repositoryId: fixture.repositoryId, scenario: 'slow', stage: 'created' });
  assert(verified.verified, 'Reloaded stash must contain original exact index/worktree content');
  const repository = await stashRepository(fixture.path);
  await click('Stashes', repository);
  const panel = await region('Stashes');
  const entries = (await api.listStashes(fixture.repositoryId)).entries;
  assert(entries.length === 1, 'Reloaded stash creation must create exactly one entry');
  await until('reloaded stash list shows completed saved entry', () => [...panel.querySelectorAll<HTMLButtonElement>('.stash-list button')].some(node => node.title === entries[0].oid));
  checks.push('Full native WebView reload during genuine running stash creation preserved the original backend operation; completed exact saved contents and one retained entry reconciled in the restarted renderer');
}

interface HunkFixture { repositoryId: string; path: string; filePath: string; firstText: string; secondText: string; message: string; head: string }
async function runHunkSmoke(checks: string[]) {
  const fixture = await invoke<HunkFixture>('native_smoke_hunk_case');
  const repository = await stashRepository(fixture.path);
  const inspectFile = async (context: 'staged' | 'unstaged') => {
    const file = await until(`${context} hunk fixture file`, () => {
      const heading = [...repository.querySelectorAll<HTMLElement>('.change-group summary')].find(node => node.textContent?.startsWith(`${context === 'staged' ? 'Staged' : 'Unstaged'} `));
      return [...(heading?.parentElement?.querySelectorAll<HTMLButtonElement>('.file-list button[title]') ?? [])].find(node => node.title === fixture.filePath);
    });
    file.click();
    return until(`structured ${context} hunk diff`, () => {
      const panel = repository.querySelector<HTMLElement>('[aria-label="File diff"]');
      return panel?.querySelector('h3')?.textContent === `Diff · ${context}` && panel.querySelector('.diff-hunk-block') ? panel : null;
    });
  };
  const mutate = async (context: 'staged' | 'unstaged', contents: string) => {
    const panel = await inspectFile(context);
    const action = context === 'staged' ? 'unstageHunk' : 'stageHunk';
    const label = context === 'staged' ? 'Unstage hunk' : 'Stage hunk';
    const count = () => [...repository.querySelectorAll('.repository-activity li')].filter(node => node.querySelector('strong')?.textContent === action.replace(/([a-z])([A-Z])/g, '$1 $2').toLowerCase() && node.querySelector('.activity-state')?.textContent === 'succeeded').length;
    const previousCount = count();
    const completed = await operation(action, fixture.repositoryId, async () => {
      const block = await until(`complete hunk containing ${contents}`, () => [...panel.querySelectorAll<HTMLElement>('.diff-hunk-block')].find(node => node.querySelector('.unified-diff, .split-diff')?.textContent?.includes(contents)));
      await click(label, block);
    });
    assert(completed.state === 'succeeded', `${label} failed: ${JSON.stringify(completed.error) ?? completed.reason}`);
    await until(`renderer reconciles ${action}`, () => count() > previousCount);
    await button('Refresh', repository);
    await refreshRepository();
  };
  const verify = async (stage: string) => {
    const result = await invoke<{ verified: boolean }>('native_smoke_hunk_verify', { repositoryId: fixture.repositoryId, stage });
    assert(result.verified, `Partial staging fixture failed ${stage} content verification`);
  };
  const draft = await until('partial commit draft', () => repository.querySelector<HTMLTextAreaElement>('#commit-message'));
  type(draft, fixture.message);
  const splitWorking = await inspectFile('unstaged');
  await chooseDiffLayout(splitWorking, 'split');
  await click('Next hunk', splitWorking);
  assert(document.activeElement?.hasAttribute('data-diff-hunk'), 'Hunk navigation must focus a hunk header');
  await click('Next hunk', splitWorking);
  assert(splitWorking.querySelectorAll('.diff-hunk-block')[1]?.contains(document.activeElement), 'Next hunk must reach the second actual block');
  await click('Previous hunk', splitWorking);
  assert(splitWorking.querySelectorAll('.diff-hunk-block')[0]?.contains(document.activeElement), 'Previous hunk must return to the first block');
  checks.push('Native split working diff saves layout and navigates actual hunk headers before unchanged-token staging/index verification');
  await mutate('unstaged', fixture.firstText);
  await verify('stagedFirst');
  const staged = await inspectFile('staged');
  assert(staged.textContent?.includes(fixture.firstText) && !staged.textContent.includes(fixture.secondText), 'Staged diff must contain only the chosen first edit');
  checks.push('Native Stage hunk used the selected complete block; exact first-only index and unchanged tracked/untracked working bytes verified, with staged-only diff inspection');
  await mutate('unstaged', fixture.secondText);
  await verify('stagedBoth');
  await mutate('staged', fixture.secondText);
  await verify('stagedFirst');
  assert(repository.querySelector<HTMLTextAreaElement>('#commit-message')?.value === fixture.message, 'Hunk actions must retain the commit draft');
  checks.push('Native second hunk staging then Unstage hunk preserved the other staged hunk; exact intermediate index and all working bytes verified and commit draft retained');
  const committed = await operation('commit', fixture.repositoryId, () => click(/^Commit staged changes \(/, repository));
  assert(committed.state === 'succeeded', `Partial commit failed: ${JSON.stringify(committed.error) ?? committed.reason}`);
  await verify('committed');
  await until('partial commit reconciled in renderer', () => [...repository.querySelectorAll('.repository-activity li')].some(node => node.querySelector('strong')?.textContent === 'commit' && node.querySelector('.activity-state')?.textContent === 'succeeded'));
  await button('Refresh', repository);
  await refreshRepository();
  const remaining = await inspectFile('unstaged');
  assert(remaining.textContent?.includes(fixture.secondText) && !remaining.textContent.includes(fixture.firstText), 'Remaining working diff must contain only the uncommitted second edit');
  checks.push('Native partial commit contains only the first hunk with exact parent/message; second edit remains uncommitted and visible, unrelated index and every working-file byte preserved');
  await chooseDiffLayout(remaining, 'unified');
}

interface RebaseFixture { repositoryId: string; path: string; originalOid: string; targetOid: string; scenario: string }
async function runRebaseSmoke(checks: string[]) {
  for (const scenario of ['success', 'conflictContinue', 'conflictSkip', 'conflictAbort'] as const) {
    const fixture = await invoke<RebaseFixture>('native_smoke_rebase_case', { scenario });
    const repository = await stashRepository(fixture.path);
    const controls = await until('rebase branch controls', () => repository.querySelector<HTMLDetailsElement>('.rebase-controls'));
    await click('History', repository);
    controls.open = true;
    controls.dispatchEvent(new Event('toggle'));
    const select = await until('rebase target selector', () => controls.querySelector<HTMLSelectElement>('select[aria-label="Rebase onto branch"]'));
    await until('local main rebase target', () => select.querySelector('option[value="refs/heads/main"]'));
    select.value = 'refs/heads/main';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    const before = await api.getStatus(fixture.repositoryId);
    assert(before.head.kind === 'branch' && before.head.oid === fixture.originalOid && before.gitState === 'idle', 'Selecting the rebase target must not change current HEAD');
    const started = await operation('startRebase', fixture.repositoryId, async () => {
      await click('Preview rebase', controls);
      const preview = await until('exact rebase preview', () => repository.querySelector<HTMLDialogElement>('dialog[open]'));
      assert(preview.textContent?.includes(fixture.originalOid) && preview.textContent.includes(fixture.targetOid) && preview.textContent.includes('Rebase source'), 'Rebase preview must show current/target OIDs and source commit');
      await click('Run rebase branch', preview);
    });
    assert(started.state === (scenario === 'success' ? 'succeeded' : 'stopped'), `Rebase ${scenario} start: ${started.state}, ${started.error?.message ?? started.reason}`);
    await refreshRepository();
    if (scenario !== 'success') {
      const notice = await region('Git operation');
      assert(notice.textContent?.includes('Rebase in progress') && notice.textContent.includes(fixture.originalOid) && notice.textContent.includes('conflict.txt'), 'Stopped rebase must show original branch, source, and unresolved file');
      assert(notice.textContent?.includes('ours is the chosen base'), 'Rebase conflict sides must be explained');
      if (scenario === 'conflictContinue') {
        await invoke('native_smoke_resolve_conflicts', { repositoryId: fixture.repositoryId });
        await refreshRepository();
        const staged = await operation('stage', fixture.repositoryId, async () => {
          const row = await until('rebase conflict row', () => [...repository.querySelectorAll<HTMLButtonElement>('.changes-workspace .file-preview')].find(node => node.title === 'conflict.txt')?.closest('li'));
          await click('Stage resolution', row);
        });
        assert(staged.state === 'succeeded', 'Rebase resolution stage must succeed');
        await refreshRepository();
        const continued = await cherryCommand(repository, fixture.repositoryId, 'continueRebase', 'Continue rebase', 'Run continue rebase');
        assert(continued.state === 'succeeded', `Rebase Continue failed: ${continued.error?.message ?? continued.reason}`);
      } else {
        const kind = scenario === 'conflictSkip' ? 'skipRebase' : 'abortRebase';
        const label = scenario === 'conflictSkip' ? 'Skip commit' : 'Abort rebase';
        const confirm = `Run ${scenario === 'conflictSkip' ? 'skip commit' : 'abort rebase'}`;
        const result = await operation(kind, fixture.repositoryId, async () => {
          await click(label, notice);
          const preview = await until(`${label} confirmation`, () => repository.querySelector<HTMLDialogElement>('dialog[open]'));
          assert(preview.textContent?.includes(scenario === 'conflictSkip' ? 'omitted' : 'restore original HEAD'), 'Rebase recovery confirmation must explain its effect');
          await click(confirm, preview);
        });
        assert(result.state === 'succeeded', `Rebase ${label} failed: ${result.error?.message ?? result.reason}`);
      }
    }
    const verified = await invoke<{ verified: boolean }>('native_smoke_rebase_verify', { repositoryId: fixture.repositoryId, scenario });
    assert(verified.verified, `Native rebase ${scenario} Git verification failed`);
    checks.push(`Native rebase ${scenario}: reviewed exact branch/target/source, operation state, and final refs/files/backup through actual DOM and IPC`);
  }
}

async function startRunningOperationReload(checks: string[]) {
  const repository = await region('Repository');
  await refreshRepository();
  await click('History', repository);
  await click('View preferences', repository);
  const panel = await until('View preferences form opens', () => {
    const node = repository.querySelector<HTMLElement>('[role="region"][aria-label="View preferences"]');
    return node && visible(node) ? node : null;
  });
  const opening = panel.querySelector<HTMLSelectElement>('[aria-label="Opening view"]')!;
  opening.value = 'history'; opening.dispatchEvent(new Event('change', { bubbles: true }));
  const layout = panel.querySelector<HTMLSelectElement>('[aria-label="Preferred diff layout"]')!;
  layout.value = 'split'; layout.dispatchEvent(new Event('change', { bubbles: true }));
  const details = panel.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
  if (details.checked) details.click();
  for (const [name, value] of [['Changed files width', '400'], ['Branches width', '230'], ['History width', '420']]) {
    type(panel.querySelector<HTMLInputElement>(`input[aria-label="${name}"]`)!, value);
  }
  await click('View preferences', repository);
  await until('view widths applied before reload', () => repository.style.getPropertyValue('--history-width') === '420px');
  const divider = repository.querySelector<HTMLElement>('[aria-label="Resize history panel"]')!;
  divider.focus(); divider.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
  await until('keyboard width saved', () => repository.style.getPropertyValue('--history-width') === '430px'
    && JSON.parse(localStorage.getItem(VIEW_STORAGE_KEY)!).preferences.historyWidth === 430);
  checks.push('Native View menu saved opening History, hidden commit details and three preferred widths; keyboard divider increment applied without Git mutation');
  const fixture = await invoke<{ repositoryId: string; operationId: string; path: string }>('native_smoke_slow_commit');
  await until('slow commit actually running before full reload', async () => {
    const operation = (await api.getOperations()).find(item => item.operationId === fixture.operationId);
    assert(!operation || operation.state === 'queued' || operation.state === 'running', `Slow fixture completed before reload could be tested: ${operation?.state}`);
    return operation?.state === 'running';
  });
  const marker: ReloadMarker = { version: 1, ...fixture, checks: [...checks] };
  sessionStorage.setItem(reloadKey, JSON.stringify(marker));
  assert(sessionStorage.getItem(reloadKey), 'Reload checkpoint must be retained in sessionStorage');
  await progressWrite;
  window.location.reload();
}

async function finishRunningOperationReload(marker: ReloadMarker, checks: string[]) {
  assert(marker.version === 1 && typeof marker.repositoryId === 'string' && typeof marker.operationId === 'string', 'Native reload checkpoint must be valid');
  // The helper is idempotent: requeue this repository's navigation after React's
  // full remount, without starting another mutation or replaying an action plan.
  const fixture = await invoke<{ repositoryId: string; operationId: string; path: string }>('native_smoke_slow_commit');
  assert(fixture.repositoryId === marker.repositoryId && fixture.operationId === marker.operationId && fixture.path === marker.path, 'Reload must reconnect to the original repository and operation');
  const completed = await until('original commit completes after full reload', async () => (await api.getOperations()).find(item => item.operationId === marker.operationId && !['queued', 'running'].includes(item.state)), 120000);
  assert(completed.state === 'succeeded', `Commit did not survive WebView reload: ${completed.error?.message ?? completed.reason}`);
  const opened = await api.openRepository(marker.path);
  const repository = await until('reloaded repository view restored', () => {
    const node = document.querySelector<HTMLElement>('.repository-screen');
    return node?.querySelector('.repository-title strong')?.getAttribute('title') === opened.displayPath ? node : null;
  });
  await until('reloaded UI reconciles original operation outcome', () => repository.querySelector('.repository-activity')?.textContent?.includes('commit: succeeded'));
  const status = await api.getStatus(marker.repositoryId);
  assert(status.head.kind === 'branch' && !status.dirty && status.counts.staged === 0, 'Reloaded operation must leave its successfully committed fixture clean');
  const history = await api.getHistory(marker.repositoryId, 'HEAD');
  assert(history.commits.length >= 2 && history.commits[0].oid === status.head.oid, 'Reloaded repository must expose the actual newly committed history');
  await until('reloaded UI refreshes actual clean state', () => repository.querySelector('.repository-title')?.textContent?.includes('Clean'));
  checks.push('Full native WebView reload during a running commit preserved the original backend operation; restarted renderer queried success, restored repository navigation, and displayed committed clean state');
  assert((await button('History', repository)).getAttribute('aria-pressed') === 'true', 'Reload must use the saved opening view');
  assert(repository.style.getPropertyValue('--changes-width') === '400px' && repository.style.getPropertyValue('--branches-width') === '230px'
    && repository.style.getPropertyValue('--history-width') === '430px', 'Reload must restore preferred widths');
  await click('View preferences', repository);
  const panel = await until('restored View preferences form opens', () => {
    const node = repository.querySelector<HTMLElement>('[role="region"][aria-label="View preferences"]');
    return node && visible(node) ? node : null;
  });
  assert(panel.querySelector<HTMLInputElement>('input[type="checkbox"]')?.checked === false, 'Reload must restore hidden commit details');
  assert(panel.querySelector<HTMLSelectElement>('[aria-label="Preferred diff layout"]')?.value === 'split', 'Reload must restore side-by-side diff preference');
  await click('View preferences', repository);
  const originalWidth = window.innerWidth, originalHeight = window.innerHeight;
  const theme = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  const saved = localStorage.getItem(VIEW_STORAGE_KEY);
  try {
    await until('View preferences closed before resize', () => !visible(panel));
    const branchesDivider = repository.querySelector<HTMLElement>('[role="separator"][aria-label="Resize branches panel"]');
    if (branchesDivider && visible(branchesDivider)) branchesDivider.focus();
    if (branchesDivider && originalWidth > 1000) assert(document.activeElement === branchesDivider, `Branch divider must receive focus before resize: ${document.activeElement?.outerHTML.slice(0, 500)}`);
    await invoke('native_smoke_appearance', { theme, width: 800, height: 580 });
    await until('minimum width applied', () => Math.abs(window.innerWidth - 800) <= 5);
    await pause(100);
    const layout = repository.querySelector<HTMLElement>('.repository-panels')!;
    assert(layout.scrollWidth <= layout.clientWidth + 1, 'Preferred panels must fit at minimum window width');
    assert(localStorage.getItem(VIEW_STORAGE_KEY) === saved, 'Window fitting must not replace saved widths');
    const scopePanel = repository.querySelector<HTMLElement>('.repository-refs')!;
    const scopeTrigger = await button('History scope and branches', repository);
    if (branchesDivider && originalWidth > 1000) assert(document.activeElement === scopeTrigger, `Collapsing the focused branch divider must restore visible scope-button focus: ${document.activeElement?.outerHTML.slice(0, 500)}`);
    assert(!visible(scopePanel) && scopeTrigger.getAttribute('aria-expanded') === 'false', 'Narrow history must initially collapse navigation');
    await click('History scope and branches', repository);
    await until('compact scope opens', () => visible(scopePanel));
    await click('All branches and tags', scopePanel);
    await until('scope selection closes navigation', () => !visible(scopePanel) && document.activeElement === scopeTrigger);
    await click('Branch actions', repository);
    await click('Create branch…', repository);
    const create = scopePanel.querySelector<HTMLDetailsElement>('.create-controls')!;
    await until('compact branch menu reveals requested form', () => visible(create) && create.open && document.activeElement === create.querySelector('summary'));
    const name = create.querySelector<HTMLInputElement>('input:not([type="checkbox"])')!;
    type(name, 'compact-draft-not-created');
    name.focus();
    name.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await until('compact Escape restores focus', () => !visible(scopePanel) && document.activeElement === scopeTrigger);
    await click('History scope and branches', repository);
    assert(name.value === 'compact-draft-not-created', 'Closing compact navigation must retain branch form input');
    await click('Close history scope', scopePanel);
    assert(localStorage.getItem(VIEW_STORAGE_KEY) === saved, 'Compact navigation must preserve all saved panel preferences');
    const unchanged = await api.getStatus(marker.repositoryId);
    assert(unchanged.head.kind === 'branch' && status.head.kind === 'branch' && unchanged.head.oid === status.head.oid && unchanged.head.name === status.head.name && !unchanged.dirty, 'Compact navigation must not change Git state');
    checks.push('Native 800×580 History scope collapses, selection closes it, branch menu reveals its form, Escape returns focus and reopening retains form input without Git mutation or preference changes');
  } finally { await invoke('native_smoke_appearance', { theme, width: originalWidth, height: originalHeight }); }
  await click('View preferences', repository);
  await click('Reset view preferences', panel);
  await until('preference reset saved', () => localStorage.getItem(VIEW_STORAGE_KEY) === null);
  assert(repository.style.getPropertyValue('--history-width') === '360px', 'Reset must restore default widths');
  checks.push('Native WebView reload restored opening view/detail visibility/widths; 800×580 panels fit without rewriting preferences; Reset removed only view settings');
}

export async function runNativeSmoke(): Promise<void> {
  const checks = progressChecks();
  let result: SmokeResult;
  try {
    const reloadCheckpoint = sessionStorage.getItem(reloadKey);
    if (reloadCheckpoint) {
      const marker = JSON.parse(reloadCheckpoint) as ReloadMarker;
      assert(Array.isArray(marker.checks) && marker.checks.every(check => typeof check === 'string'), 'Reload checkpoint must retain prior verification results');
      checks.push(...marker.checks);
      await finishRunningOperationReload(marker, checks);
      sessionStorage.removeItem(reloadKey);
      await progressWrite;
      await invoke('native_smoke_report', { result: { passed: true, checks } satisfies SmokeResult });
      return;
    }
    const stashCheckpoint = sessionStorage.getItem(stashReloadKey);
    if (stashCheckpoint) {
      const marker = JSON.parse(stashCheckpoint) as ReloadMarker;
      assert(marker.version === 1 && Array.isArray(marker.checks), 'Stash reload checkpoint must be valid');
      checks.push(...marker.checks);
      await finishStashReload(marker, checks);
      sessionStorage.removeItem(stashReloadKey);
      if ((await invoke<SmokeContext>('native_smoke_context')).hunks) await runHunkSmoke(checks);
      if ((await invoke<SmokeContext>('native_smoke_context')).rebase) await runRebaseSmoke(checks);
      await startRunningOperationReload(checks);
      return;
    }
    const cherryCheckpoint = sessionStorage.getItem(cherryReloadKey);
    if (cherryCheckpoint) {
      const marker = JSON.parse(cherryCheckpoint) as CherryReloadMarker;
      assert(Array.isArray(marker.checks) && marker.checks.every(check => typeof check === 'string'), 'Cherry-pick checkpoint must retain prior checks');
      checks.push(...marker.checks);
      await finishCherryPickReload(marker, checks);
      sessionStorage.removeItem(cherryReloadKey);
      if ((await invoke<SmokeContext>('native_smoke_context')).stash) { await runStashSmoke(checks); return; }
      if ((await invoke<SmokeContext>('native_smoke_context')).hunks) await runHunkSmoke(checks);
      if ((await invoke<SmokeContext>('native_smoke_context')).rebase) await runRebaseSmoke(checks);
      await startRunningOperationReload(checks);
      return;
    }
    // Failed runs can leave test-only preferences behind. Keep reload checkpoints
    // intact above, but start a new disposable suite from its known opening view.
    localStorage.removeItem(VIEW_STORAGE_KEY);
    const context = await invoke<SmokeContext>('native_smoke_context');
    const group = await region('Repository group');
    const project = await api.openProject(context.projectPath);
    assert(group.querySelector('h1')?.textContent === project.name, 'CLI must display the requested real project');
    assert(group.querySelectorAll('.group-table-scroll tbody tr').length === project.members.length, 'Dashboard must display every project member');
    const clean = await api.openRepository(context.cleanPath);
    const dirty = await api.openRepository(context.dirtyPath);
    const cleanBefore = JSON.stringify(clean.status.head);
    const dirtyBefore = JSON.stringify({ head: dirty.status.head, counts: dirty.status.counts, dirty: dirty.status.dirty });
    assert(dirty.status.dirty, 'Dirty fixture must start dirty');
    checks.push('Cold CLI launch rendered real JSON project with all members');

    const filter = group.querySelector<HTMLInputElement>('input[type="search"]')!;
    type(filter, 'temporary-keyboard-filter');
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', ctrlKey: true, bubbles: true, cancelable: true }));
    await until('project shortcut selects filter', () => document.activeElement === filter && filter.selectionStart === 0 && filter.selectionEnd === filter.value.length);
    filter.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    await until('project Escape restores all rows', () => filter.value === '' && group.querySelectorAll('.group-table-scroll tbody tr').length === project.members.length);
    const help = await button('Keyboard shortcuts');
    help.focus(); help.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
    const reference = await until('shortcut reference accepts keyboard focus', () => {
      const node = document.querySelector<HTMLElement>('[role="region"][aria-label="Keyboard shortcuts"]');
      return node && visible(node) && document.activeElement === node ? node : null;
    });
    assert(reference.textContent?.includes('Does not fetch or pull'), 'Shortcut reference must explain refresh scope');
    reference.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    assert(document.activeElement === help, 'Escape must return focus from shortcut reference');
    checks.push('Native project primary-F focuses/selects name-path filter, Escape clears text without Git operations; keyboard opens a focused shortcut reference and Escape restores its trigger');

    const oldOperations = new Set((await api.getOperations()).map(item => item.operationId));
    await click(/^Pull all \(/, group);
    const preview = await region('Batch preview');
    assert(preview.querySelectorAll('li').length === project.members.length, 'Pull all preview must include dirty/missing candidates');
    assert(preview.textContent?.includes('Skipped'), 'Pull preview must display skipped reasons');
    await click(/^Run \d+ eligible$/, preview);
    const batch = await until('mixed batch finishes', async () => {
      const rows = (await api.getOperations()).filter(item => item.batchId && !oldOperations.has(item.operationId));
      return rows.length === project.members.length && rows.every(item => !['queued', 'running'].includes(item.state)) ? rows : null;
    }, 120000);
    assert(batch.some(item => item.repositoryId === clean.repositoryId && item.state === 'succeeded'), 'Clean repository pull must succeed');
    assert(batch.some(item => item.repositoryId === dirty.repositoryId && item.state === 'skipped'), 'Dirty repository pull must be skipped');
    assert(batch.some(item => item.state === 'failed'), 'Diverged fixture must report failure alongside success');
    const cleanAfter = await api.getStatus(clean.repositoryId);
    const dirtyAfter = await api.getStatus(dirty.repositoryId);
    assert(JSON.stringify(cleanAfter.head) !== cleanBefore, 'Clean pull must advance HEAD');
    assert(JSON.stringify({ head: dirtyAfter.head, counts: dirtyAfter.counts, dirty: dirtyAfter.dirty }) === dirtyBefore, 'Dirty repository HEAD and working state must remain unchanged');
    const activity = await region('Batch activity');
    await until('mixed outcomes render', () => ['succeeded', 'failed', 'skipped'].every(state => activity.textContent?.includes(state)));
    checks.push('DOM Pull all preview and run: clean advanced, dirty skipped unchanged, divergence failed, independent outcomes rendered');

    const cleanMember = project.members.find(member => member.kind === 'ready' && member.repositoryId === clean.repositoryId);
    assert(cleanMember, 'Clean fixture must be a group member');
    const groupSearch = group.querySelector<HTMLInputElement>('input[type="search"]');
    assert(groupSearch, 'Group search control must exist');
    type(groupSearch, cleanMember.label);
    await until('group filter applied', () => group.querySelectorAll('.group-table-scroll tbody tr').length === 1);
    const cleanRow = [...group.querySelectorAll<HTMLTableRowElement>('.group-table-scroll tbody tr')].find(row => row.querySelector('.group-path')?.getAttribute('title') === cleanMember.displayPath);
    assert(cleanRow, 'Clean member row must be rendered');
    const selectedMember = cleanRow.querySelector<HTMLInputElement>('input[type="checkbox"]');
    assert(selectedMember, 'Group member checkbox must exist');
    if (!selectedMember.checked) selectedMember.click();
    await click(cleanMember.label, cleanRow);
    const repository = await region('Repository');
    await click('History', repository);
    await until('virtualized history renders', () => repository.querySelector<HTMLButtonElement>('.history-row'));
    await checkHistoryKeyboardAndAlignment(repository, checks);
    await checkNativeAppearance(checks);
    const commitDetails = await region('Commit details');
    const historicalFile = await until('historical changed file', () => commitDetails.querySelector<HTMLButtonElement>('.file-list button'));
    historicalFile.click();
    await assertDiffVisible();
    checks.push('Opened repository from dashboard; selected real history row and rendered commit file patch');
    const historicalDiff = await region('File diff');
    await chooseDiffLayout(historicalDiff, 'split');
    const pair = historicalDiff.querySelector('.split-row')!;
    assert(pair && pair.children.length === 2, 'Historical split diff must show old/new counterparts');
    const oldBox = pair.children[0].getBoundingClientRect(), newBox = pair.children[1].getBoundingClientRect();
    assert(Math.abs(oldBox.top - newBox.top) < 1 && Math.abs(oldBox.height - newBox.height) < 1, 'Native split counterparts must align vertically');
    await click('Next hunk', historicalDiff);
    assert(document.activeElement?.hasAttribute('data-diff-hunk'), 'Historical hunk navigation must focus a header');
    assert(![...historicalDiff.querySelectorAll('button')].some(node => /^(Unstage|Stage) hunk$/.test(node.textContent ?? '')), 'Historical split diff must remain read-only');
    await chooseDiffLayout(historicalDiff, 'unified');
    checks.push('Native historical diff toggles unified/split with aligned old/new rows, keyboard hunk focus and no staging controls');

    const mergeRepo = await api.openRepository(context.mergePath);
    const mergeHistory = await api.getHistory(mergeRepo.repositoryId);
    const merge = mergeHistory.commits.find(commit => commit.parents.length > 1);
    assert(merge && merge.parents.length === 2, 'Merge fixture must contain a two-parent merge in its first page');
    for (const parent of merge.parents) {
      const details = await api.getCommit(mergeRepo.repositoryId, merge.oid, parent);
      assert(details.selectedParent === parent && details.files.length > 0, 'Each merge parent must return its own changed-file context');
      const patch = await api.getDiff({ repositoryId: mergeRepo.repositoryId, snapshotId: details.snapshotId, fileId: details.files[0].fileId, context: 'commit' });
      assert(patch.kind === 'text' && patch.text.length > 0, 'Each merge parent must have a real text diff');
    }
    checks.push('Real native IPC inspected merge commit against each parent and read corresponding patches');

    const prepared = await invoke<{ firstFile: string; secondFile: string }>('native_smoke_prepare_changes', { repositoryId: clean.repositoryId });
    await click(/^Working changes/, repository);
    await refreshRepository();
    const workingFile = await until('working file appears', () => [...repository.querySelectorAll<HTMLButtonElement>('.file-list button[title]')].find(node => node.title === prepared.firstFile));
    workingFile.click();
    await assertDiffVisible();
    for (const path of [prepared.firstFile, prepared.secondFile]) {
      const checkbox = await until(`bulk selection for ${path}`, () => repository.querySelector<HTMLInputElement>(`input[aria-label="Select unstaged file ${path}"]:not(:disabled)`));
      checkbox.click();
    }
    const stage = await operation('stage', clean.repositoryId, () => click('Stage selected (2)', repository));
    assert(stage.state === 'succeeded', `Stage failed: ${stage.error?.message ?? stage.reason}`);
    await until('bulk selection clears after stage', () => !repository.querySelector('[aria-label="Selected file actions"]'));
    const stagedFiles = await api.getChanges(clean.repositoryId);
    assert(stagedFiles.files.filter(file => file.indexStatus === 'M').length === 2, 'Both checked files must be staged');
    const secondCheckbox = await until('second file staged checkbox', () => repository.querySelector<HTMLInputElement>(`input[aria-label="Select staged file ${prepared.secondFile}"]:not(:disabled)`));
    secondCheckbox.click();
    const unstage = await operation('unstage', clean.repositoryId, () => click('Unstage selected (1)', repository));
    assert(unstage.state === 'succeeded', 'Checked staged file must unstage successfully');
    checks.push('Native multi-file selection staged both exact files, cleared after refresh, and unstaged only the checked second file before a first-file-only commit');
    await until('stage reflected in commit control', () => [...repository.querySelectorAll<HTMLButtonElement>('button')].find(node => node.textContent === 'Commit staged changes (1)'));
    const message = `Native smoke staged-only commit ${Date.now()}`;
    const messageInput = await focusCommitEditor(repository);
    checks.push('Native commit editor accepts focus with a visible outline');
    type(messageInput, message);
    const committed = await operation('commit', clean.repositoryId, () => click('Commit staged changes (1)', repository));
    assert(committed.state === 'succeeded', `Commit failed: ${committed.error?.message ?? committed.reason}`);
    const committedStatus = await api.getStatus(clean.repositoryId);
    assert(committedStatus.head.kind !== 'unborn', 'Commit must create a real HEAD');
    const committedDetails = await api.getCommit(clean.repositoryId, committedStatus.head.oid);
    assert(committedDetails.message.trim() === message, 'Real commit message must match the entered draft');
    assert(committedDetails.files.length === 1 && committedDetails.files[0].displayPath === prepared.firstFile, 'Commit must contain only the one staged fixture file');
    const remaining = await api.getChanges(clean.repositoryId);
    assert(remaining.files.some(file => file.displayPath === prepared.secondFile), 'Second file must remain uncommitted');
    await until('successful draft clears', () => messageInput.value === '');
    checks.push('DOM inspected working patch, staged one of two files, committed only selected file, and cleared successful draft');

    await refreshRepository();
    const pushPanel = repository.querySelector<HTMLElement>('.push-controls');
    assert(pushPanel, 'Push controls must be present');
    await click('Push…', repository);
    const push = await operation('push', clean.repositoryId, async () => {
      await click('Preview push', pushPanel);
      const dialog = await until('push review modal', () => repository.querySelector<HTMLDialogElement>('dialog[open]'));
      assert(dialog.textContent?.includes('Destination:'), 'Push preview must display the resolved destination');
      await click('Run push', dialog);
    });
    assert(push.state === 'succeeded', `Push to local fixture remote failed: ${push.error?.message ?? push.reason}`);
    checks.push('DOM push preview displayed destination and real push to fixture remote succeeded');

    await invoke('native_smoke_fail_hook', { repositoryId: clean.repositoryId });
    await refreshRepository();
    const hookStage = await operation('stage', clean.repositoryId, () => stageFile(prepared.secondFile));
    assert(hookStage.state === 'succeeded', 'Stage before failing hook must succeed');
    await until('hook-test stage reflected', () => [...repository.querySelectorAll<HTMLButtonElement>('button')].find(node => node.textContent === 'Commit staged changes (1)'));
    const failedMessage = 'Retain this native smoke draft after hook failure';
    type(messageInput, failedMessage);
    const failed = await operation('commit', clean.repositoryId, () => click('Commit staged changes (1)', repository));
    assert(failed.state === 'failed', 'Rejecting native Git hook must fail commit');
    await until('hook failure renders', () => [...document.querySelectorAll('[role="alert"]')].some(node => visible(node) && node.textContent?.trim()));
    assert(messageInput.value === failedMessage, 'Draft must survive actual hook failure');
    assert(JSON.stringify((await api.getStatus(clean.repositoryId)).head) === JSON.stringify(committedStatus.head), 'Failed commit must not advance HEAD');
    await click('Repository activity', repository);
    const activityDrawer = await region('Repository activity details');
    const activityClose = await button('Close activity', activityDrawer);
    await until('activity opening focuses close', () => document.activeElement === activityClose);
    const activityFilter = activityDrawer.querySelector<HTMLSelectElement>('[aria-label="Activity filter"]')!;
    activityFilter.value = 'attention'; activityFilter.dispatchEvent(new Event('change', { bubbles: true }));
    const failureRow = await until('failed commit appears in filtered drawer', () => [...activityDrawer.querySelectorAll('li')].find(node => node.textContent?.includes(failed.operationId)));
    assert(failureRow.textContent?.includes(failed.error?.message ?? ''), 'Activity must explain the real failure');
    failureRow.querySelector('summary')!.click();
    assert(failureRow.querySelector('details')!.open, 'Native operation details must expand');
    if (failed.error?.details) assert(failureRow.querySelector('pre')?.textContent === failed.error.details, 'Activity must show exact backend diagnostic text');
    activityClose.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    await until('activity Escape restores visible trigger', () => !visible(activityDrawer) && document.activeElement === repository.querySelector('[aria-label="Repository activity"]'));
    assert(messageInput.value === failedMessage && JSON.stringify((await api.getStatus(clean.repositoryId)).head) === JSON.stringify(committedStatus.head), 'Inspecting activity must preserve draft and HEAD');
    checks.push('Native read-only activity drawer filters to actual failed commit, expands backend diagnostics, and restores focus with Escape without changing draft or HEAD');
    await click(`Back to ${project.name}`);
    const returnedGroup = await region('Repository group');
    assert(returnedGroup.querySelector<HTMLInputElement>('input[type="search"]')?.value === cleanMember.label, 'Group search must survive repository navigation');
    const restoredSelection = [...returnedGroup.querySelectorAll<HTMLTableRowElement>('.group-table-scroll tbody tr')].find(row => row.querySelector('.group-path')?.getAttribute('title') === cleanMember.displayPath)?.querySelector<HTMLInputElement>('input[type="checkbox"]');
    assert(restoredSelection?.checked, 'Group checked selection must survive repository navigation');
    checks.push('Returning from repository preserved the group search filter and checked member');
    await click('Clear filters', returnedGroup);
    await click('Refresh', returnedGroup);
    await until('returned dashboard refreshed', () => [...returnedGroup.querySelectorAll<HTMLTableRowElement>('.group-table-scroll tbody tr')].some(row => row.textContent?.includes(cleanMember.label) && row.textContent.includes('1 staged')));
    const returnedRow = [...returnedGroup.querySelectorAll<HTMLTableRowElement>('.group-table-scroll tbody tr')].find(row => row.querySelector('.group-path')?.getAttribute('title') === cleanMember.displayPath);
    assert(returnedRow, 'Original group row must remain present');
    await click(cleanMember.label, returnedRow);
    const retained = await until('retained draft after navigation', () => document.querySelector<HTMLTextAreaElement>('#commit-message'));
    assert(retained.value === failedMessage, 'Draft must survive leaving and reopening repository');
    await until('operation state reconciled after repository remount', () => document.querySelector('.repository-activity')?.textContent?.includes('commit: failed'));
    checks.push('Repository remount queried real operation snapshots and restored failed commit activity without relying on prior events');
    checks.push('Real rejecting hook failed without moving HEAD; draft survived failure and navigation; group status refreshed');

    const forward = (mode: 'repository' | 'welcome' | 'invalid' | 'project') => invoke<{ exitCode: number; stdout: string; stderr: string }>('native_smoke_forward_launch', { mode });
    const invalidLaunch = await forward('invalid');
    assert(invalidLaunch.exitCode !== 0, 'Invalid explicit repository CLI must exit nonzero');
    assert(document.querySelector('.repository-screen') && document.querySelector<HTMLTextAreaElement>('#commit-message')?.value === failedMessage, 'Invalid CLI must leave the current repository and draft untouched');
    checks.push('Warm invalid explicit repository CLI returned nonzero without changing the open repository');

    const warmLaunchStarted = performance.now();
    const welcomeLaunch = await forward('welcome');
    assert(welcomeLaunch.exitCode === 0, `Warm welcome launch failed: ${welcomeLaunch.stderr}`);
    await until('warm no-argument launch outside Git shows welcome', () => {
      const welcome = document.querySelector<HTMLElement>('.welcome');
      return welcome && visible(welcome) ? welcome : null;
    });
    checks.push(`Warm no-argument CLI from outside Git forwarded to welcome; CLI subprocess through visible welcome render ${(performance.now() - warmLaunchStarted).toFixed(1)} ms`);

    const repositoryLaunch = await forward('repository');
    assert(repositoryLaunch.exitCode === 0, `Warm repository launch failed: ${repositoryLaunch.stderr}`);
    const reopened = await region('Repository');
    assert(reopened.querySelector('.repository-title strong')?.getAttribute('title') === clean.displayPath, 'Warm repo . must resolve against the second process cwd');
    const forwardedDraft = await until('draft after warm repository launch', () => reopened.querySelector<HTMLTextAreaElement>('#commit-message'));
    assert(forwardedDraft.value === failedMessage, 'Canonical repository draft must survive warm CLI navigation');
    checks.push('Warm repo . used invoking cwd, selected the existing canonical repository, and retained its draft');

    const projectLaunch = await forward('project');
    assert(projectLaunch.exitCode === 0, `Warm project launch failed: ${projectLaunch.stderr}`);
    const forwardedGroup = await region('Repository group');
    await until('warm relative project launch displays correct group', () => forwardedGroup.querySelector('h1')?.textContent === project.name);
    assert(forwardedGroup.querySelectorAll('.group-table-scroll tbody tr').length === project.members.length, 'Warm project launch must retain all fixture members');
    checks.push('Warm group CLI resolved relative project path from invoking cwd and displayed the requested group');
    if (context.m2) await runMergeSmoke(checks);
    if (context.m3) await runCombineSmoke(checks);
    if (context.performance) await runPerformanceSmoke(checks);
    if (context.discovery) await runDiscoverySmoke(checks);
    if (context.discovery) { await runGroupStatusSmoke(checks); await runGroupSwitchSmoke(checks); }
    if (context.historySearch) await runHistorySearchSmoke(checks);
    if (context.remoteCheckout) await runRemoteCheckoutSmoke(checks);
    if (context.cherryPick) { await runCherryPickSmoke(checks); return; }
    if (context.stash) { await runStashSmoke(checks); return; }
    if (context.hunks) await runHunkSmoke(checks);
    if (context.rebase) await runRebaseSmoke(checks);
    await startRunningOperationReload(checks);
    return; // The reloaded renderer resumes above and submits the final report.
  } catch (cause) {
    sessionStorage.removeItem(reloadKey);
    sessionStorage.removeItem(cherryReloadKey);
    sessionStorage.removeItem(stashReloadKey);
    result = { passed: false, checks, error: errorMessage(cause), body: document.body.innerText.slice(0, 16000) };
  }
  await progressWrite;
  await invoke('native_smoke_report', { result });
}
