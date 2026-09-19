// Imported explicitly by the coordinator's native-smoke build only. This drives
// the actual React DOM and Tauri commands: no renderer or Git mocks are used.
import { invoke } from '@tauri-apps/api/core';
import { api, errorMessage } from '../../api/client';
import type { OperationResult } from '../../api/types';

interface SmokeContext { projectPath: string; cleanPath: string; dirtyPath: string; mergePath: string; m2?: boolean }
interface SmokeResult { passed: boolean; checks: string[]; error?: string; body?: string }
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

export async function runNativeSmoke(): Promise<void> {
  const checks: string[] = [];
  let result: SmokeResult;
  try {
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
    const cleanRow = [...group.querySelectorAll<HTMLTableRowElement>('.group-table-scroll tbody tr')].find(row => row.querySelector('.group-path')?.getAttribute('title') === cleanMember.displayPath);
    assert(cleanRow, 'Clean member row must be rendered');
    await click(cleanMember.label, cleanRow);
    const repository = await region('Repository');
    await until('virtualized history renders', () => repository.querySelector<HTMLButtonElement>('.history-row'));
    repository.querySelector<HTMLButtonElement>('.history-row')!.click();
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
    const messageInput = await until('commit message editor', () => repository.querySelector<HTMLTextAreaElement>('#commit-message'));
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
    await click('Refresh', returnedGroup);
    await until('returned dashboard refreshed', () => [...returnedGroup.querySelectorAll<HTMLTableRowElement>('.group-table-scroll tbody tr')].some(row => row.textContent?.includes(cleanMember.label) && row.textContent.includes('1 staged')));
    const returnedRow = [...returnedGroup.querySelectorAll<HTMLTableRowElement>('.group-table-scroll tbody tr')].find(row => row.querySelector('.group-path')?.getAttribute('title') === cleanMember.displayPath);
    assert(returnedRow, 'Original group row must remain present');
    await click(cleanMember.label, returnedRow);
    const retained = await until('retained draft after navigation', () => document.querySelector<HTMLTextAreaElement>('#commit-message'));
    assert(retained.value === failedMessage, 'Draft must survive leaving and reopening repository');
    checks.push('Real rejecting hook failed without moving HEAD; draft survived failure and navigation; group status refreshed');

    const forward = (mode: 'repository' | 'welcome' | 'invalid' | 'project') => invoke<{ exitCode: number; stdout: string; stderr: string }>('native_smoke_forward_launch', { mode });
    const invalidLaunch = await forward('invalid');
    assert(invalidLaunch.exitCode !== 0, 'Invalid explicit repository CLI must exit nonzero');
    assert(document.querySelector('.repository-screen') && document.querySelector<HTMLTextAreaElement>('#commit-message')?.value === failedMessage, 'Invalid CLI must leave the current repository and draft untouched');
    checks.push('Warm invalid explicit repository CLI returned nonzero without changing the open repository');

    const welcomeLaunch = await forward('welcome');
    assert(welcomeLaunch.exitCode === 0, `Warm welcome launch failed: ${welcomeLaunch.stderr}`);
    await until('warm no-argument launch outside Git shows welcome', () => {
      const welcome = document.querySelector<HTMLElement>('.welcome');
      return welcome && visible(welcome) ? welcome : null;
    });
    checks.push('Warm no-argument CLI from outside Git forwarded to welcome');

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
    result = { passed: true, checks };
  } catch (cause) {
    result = { passed: false, checks, error: errorMessage(cause), body: document.body.innerText.slice(0, 16000) };
  }
  await invoke('native_smoke_report', { result });
}
