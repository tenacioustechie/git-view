// Imported explicitly by the coordinator's native-smoke build only. This drives
// the actual React DOM and Tauri commands: no renderer or Git mocks are used.
import { invoke } from '@tauri-apps/api/core';
import { api, errorMessage } from '../../api/client';
import type { OperationResult } from '../../api/types';

interface SmokeContext { projectPath: string; cleanPath: string; dirtyPath: string; mergePath: string; m2?: boolean; m3?: boolean; performance?: boolean; discovery?: boolean }
interface SmokeResult { passed: boolean; checks: string[]; error?: string; body?: string }
interface ReloadMarker { version: 1; repositoryId: string; operationId: string; path: string; checks: string[] }
const reloadKey = 'git-view.native-smoke.reload.v1';
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
    const text = node.textContent?.trim() ?? '';
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
  const row = await until(`stageable row ${path}`, () => [...document.querySelectorAll<HTMLButtonElement>('.repository-details .file-list button[title]')]
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
    return buttons.some(node => node.textContent === 'Refresh' && !node.disabled);
  });
}
async function assertDiffVisible() {
  const diff = await region('File diff');
  await until('text diff renders', () => diff.querySelector('.unified-diff'));
  assert(diff.querySelector('.unified-diff')!.textContent?.trim(), 'Selected file must render an actual patch');
}

async function focusCommitEditor(repository: HTMLElement) {
  let diagnostics = '';
  let stableFrames = 0;
  let previous: HTMLTextAreaElement | null = null;
  try {
    return await until('stable commit editor focus and visible outline', async () => {
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
  scroll.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
  await until('ArrowDown selects next actual commit', () => scroll.querySelector<HTMLButtonElement>('.history-row[aria-pressed="true"]')?.title.split('\n')[0] === rows[1].title.split('\n')[0]);
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
    await click('Combine commits…', repository);
    const panel = await region('Combine commits');
    const count = await until('combine count', () => panel.querySelector<HTMLInputElement>('[aria-label="Newest commits to combine"]'));
    type(count, '1');
    await until('one-commit selection rejected', () => [...panel.querySelectorAll<HTMLButtonElement>('button')].some(node => node.textContent === 'Preview combine commits' && node.disabled));
    type(count, '2');
    const selection = panel.querySelector('[aria-label="Commits to replace"]');
    await until('exact selected suffix rendered', () => fixture.commitOids.every(oid => selection?.textContent?.includes(oid)));
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
  const scroll = await until('first 200 history rows laid out', () => {
    const node = repository.querySelector<HTMLDivElement>('.history-list');
    const layout = node?.firstElementChild as HTMLElement | null;
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
  assert(saved.members.length === 1 && saved.members[0].displayPath === selected[0].path, 'Saved subset must resolve to exactly the checked repository');
  checks.push('Discovery DOM-selected subset saved through real create_project IPC and reopened through CLI with exactly one member; native OS save picker was not automated');

  const expanded = await fixtureAction('add');
  const addedPath = expanded.candidatePaths.find(path => !fixture.candidatePaths.includes(path));
  assert(addedPath && expanded.candidatePaths.length === 3, 'Rescan fixture must add exactly one repository');
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
  assert(updated.members.length === 2 && updated.members.some(member => member.displayPath === selected[0].path) && updated.members.some(member => member.displayPath === addedPath), 'Explicit DOM save must preserve original member and append only selected addition');
  assert(updated.revision !== saved.revision, 'Explicit discovery-addition save must produce a new project revision');
  checks.push('Existing group discovery excluded duplicates and proposed unchecked addition; rescans/cancel/draft cancel preserved saved membership, explicit DOM Save project appended only chosen repository');
}

async function startRunningOperationReload(checks: string[]) {
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
    await until('virtualized history renders', () => repository.querySelector<HTMLButtonElement>('.history-row'));
    await checkHistoryKeyboardAndAlignment(repository, checks);
    await checkNativeAppearance(checks);
    const commitDetails = await region('Commit details');
    const historicalFile = await until('historical changed file', () => commitDetails.querySelector<HTMLButtonElement>('.file-list button'));
    historicalFile.click();
    await assertDiffVisible();
    checks.push('Opened repository from dashboard; selected real history row and rendered commit file patch');

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
    const stage = await operation('stage', clean.repositoryId, () => stageFile(prepared.firstFile));
    assert(stage.state === 'succeeded', `Stage failed: ${stage.error?.message ?? stage.reason}`);
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
    const pushPanel = repository.querySelector<HTMLDetailsElement>('.push-controls');
    assert(pushPanel, 'Push controls must be present');
    if (!pushPanel.open) pushPanel.querySelector('summary')!.click();
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
    await click('← Back', repository);
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
    await startRunningOperationReload(checks);
    return; // The reloaded renderer resumes above and submits the final report.
  } catch (cause) {
    sessionStorage.removeItem(reloadKey);
    result = { passed: false, checks, error: errorMessage(cause), body: document.body.innerText.slice(0, 16000) };
  }
  await progressWrite;
  await invoke('native_smoke_report', { result });
}
