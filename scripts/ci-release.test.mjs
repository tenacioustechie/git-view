import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { assemble, checkVersion, collect, draft, packages, releaseBinary } from './ci-release.mjs';

const revision = 'a'.repeat(40);
const tag = 'v0.1.0';
const put = (file, value) => writeFileSync(file, JSON.stringify(value));
const readJson = (file) => JSON.parse(readFileSync(file, 'utf8'));

function fixture(t, version = '0.1.0') {
  const root = mkdtempSync(join(tmpdir(), 'git-view-packaging-test-'));
  t.after(() => rmSync(root, { recursive: true }));
  mkdirSync(join(root, 'src-tauri'), { recursive: true });
  put(join(root, 'package.json'), { version });
  put(join(root, 'src-tauri/tauri.conf.json'), { version });
  writeFileSync(join(root, 'src-tauri/Cargo.toml'), `[package]\nname = "git-view"\nversion = "${version}"\n\n[lib]\nname = "git_view"\n`);
  writeFileSync(join(root, 'src-tauri/Cargo.lock'), `[[package]]\nname = "git-view"\nversion = "${version}"\n`);
  return root;
}

function sourcePackages(root) {
  for (const [directory, suffix] of Object.values(packages).flat()) {
    const output = join(root, 'src-tauri/target/release/bundle', directory);
    mkdirSync(output, { recursive: true });
    writeFileSync(join(output, `Git View_0.1.0${suffix}`), `package fixture ${directory}`);
  }
}

function downloaded(root) {
  sourcePackages(root);
  const input = join(root, 'downloaded-packages');
  for (const platform of Object.keys(packages)) collect(root, platform, revision, join(input, `git-view-${platform}`));
  return input;
}

test('versions agree across manifests; exact stable and prerelease tags accepted', (t) => {
  assert.equal(checkVersion(fixture(t), tag), '0.1.0');
  assert.equal(checkVersion(fixture(t, '1.2.3-beta.1'), 'v1.2.3-beta.1'), '1.2.3-beta.1');
});

test('release smoke uses the retained target executable after packaging', () => {
  assert.equal(releaseBinary('/repo', 'darwin'), join('/repo', 'src-tauri/target/release/git-view'));
  assert.equal(releaseBinary('/repo', 'linux'), join('/repo', 'src-tauri/target/release/git-view'));
  assert.equal(releaseBinary('/repo', 'win32'), join('/repo', 'src-tauri/target/release/git-view.exe'));
});

test('rejects tag mismatches, unsafe versions and stale individual manifests', (t) => {
  assert.throws(() => checkVersion(fixture(t), 'v9.0.0'), /Tag must exactly match/);
  assert.throws(() => checkVersion(fixture(t, '../bad')));
  for (const file of ['package.json', 'src-tauri/tauri.conf.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock']) {
    const root = fixture(t);
    const path = join(root, file);
    writeFileSync(path, readFileSync(path, 'utf8').replace('0.1.0', '0.2.0'));
    assert.throws(() => checkVersion(root));
  }
});

test('collects only exact requested formats with normalized names, SHA and source identity', (t) => {
  const root = fixture(t);
  sourcePackages(root);
  for (const platform of Object.keys(packages)) {
    const output = join(root, platform);
    const manifest = collect(root, platform, revision, output);
    assert.equal(manifest.revision, revision);
    assert.equal(manifest.files.length, platform === 'linux-x64' ? 2 : 1);
    for (const file of manifest.files) {
      assert.match(file.name, new RegExp(`^Git-View_0.1.0_${platform}`));
      assert.equal(file.sha256, createHash('sha256').update(readFileSync(join(output, file.name))).digest('hex'));
    }
    assert.throws(() => collect(root, platform, revision, output), /Refusing nonempty/);
  }
});

test('collection rejects missing, ambiguous, empty and unknown package inputs', (t) => {
  const root = fixture(t);
  assert.throws(() => collect(root, 'linux-x64', revision));
  assert.throws(() => collect(root, '../linux', revision), /Unknown package/);
  sourcePackages(root);
  const extra = join(root, 'src-tauri/target/release/bundle/dmg/old.dmg');
  writeFileSync(extra, 'old');
  assert.throws(() => collect(root, 'macos-arm64', revision), /exactly one/);
  rmSync(extra);
  writeFileSync(join(root, 'src-tauri/target/release/bundle/dmg/Git View_0.1.0.dmg'), '');
  assert.throws(() => collect(root, 'macos-arm64', revision), /Empty package/);
});

test('assembly requires all four platforms and produces checksums for five packages and four manifests', (t) => {
  const root = fixture(t);
  const input = downloaded(root);
  const output = join(root, 'release-assets');
  assert.equal(assemble(root, input, output, revision, tag), '0.1.0');
  assert.equal(readdirSync(output).length, 10);
  for (const line of readFileSync(join(output, 'SHA256SUMS.txt'), 'utf8').trim().split('\n')) {
    const [hash, name] = line.split('  ');
    assert.equal(hash, createHash('sha256').update(readFileSync(join(output, name))).digest('hex'));
  }
  assert.throws(() => assemble(root, input, output, revision, tag), /Refusing nonempty/);
});

for (const fault of ['missing platform', 'extra platform', 'wrong revision', 'wrong version', 'tamper', 'extra file', 'traversal', 'duplicate']) {
  test(`assembly rejects ${fault} without emitting release assets`, (t) => {
    const root = fixture(t);
    const input = downloaded(root);
    const directory = join(input, 'git-view-linux-x64');
    const metadata = join(directory, 'manifest-linux-x64.json');
    const manifest = readJson(metadata);
    if (fault === 'missing platform') rmSync(directory, { recursive: true });
    if (fault === 'extra platform') mkdirSync(join(input, 'unexpected'));
    if (fault === 'wrong revision') manifest.revision = 'b'.repeat(40);
    if (fault === 'wrong version') manifest.version = '9.9.9';
    if (fault === 'tamper') writeFileSync(join(directory, manifest.files[0].name), 'tamper');
    if (fault === 'extra file') writeFileSync(join(directory, 'unexpected.exe'), 'bad');
    if (fault === 'traversal') manifest.files[0].name = '../escape.deb';
    if (fault === 'duplicate') manifest.files[1] = manifest.files[0];
    if (!['missing platform'].includes(fault)) put(metadata, manifest);
    assert.throws(() => assemble(root, input, join(root, 'release-assets'), revision, tag));
    assert(!readdirSync(root).includes('release-assets'));
  });
}

const releaseEnv = (version = '0.1.0') => ({ GITHUB_EVENT_NAME: 'push', GITHUB_REF_TYPE: 'tag', GITHUB_REF_NAME: `v${version}`, GITHUB_SHA: revision });

test('draft uses verified existing tag, no overwrite/publish flag, and all verified assets', (t) => {
  const root = fixture(t);
  assemble(root, downloaded(root), join(root, 'release-assets'), revision, tag);
  const calls = [];
  draft(root, releaseEnv(), (command, args) => {
    assert.equal(command, 'gh');
    calls.push(args);
    return JSON.stringify({ object: { type: 'commit', sha: revision } });
  });
  const command = calls.at(-1);
  assert.deepEqual(command.slice(0, 3), ['release', 'create', tag]);
  assert(command.includes('--draft') && command.includes('--verify-tag'));
  assert(!command.includes('--clobber') && !command.includes('--prerelease'));
  assert.equal(command.filter((arg) => arg.startsWith(join(root, 'release-assets'))).length, 10);
});

test('annotated prerelease tag is peeled and draft marked prerelease', (t) => {
  const root = fixture(t, '1.2.3-beta.1');
  assemble(root, downloaded(root), join(root, 'release-assets'), revision, 'v1.2.3-beta.1');
  const calls = [];
  draft(root, releaseEnv('1.2.3-beta.1'), (_, args) => {
    calls.push(args);
    return JSON.stringify({ object: calls.length === 1 ? { type: 'tag', sha: 'b'.repeat(40) } : { type: 'commit', sha: revision } });
  });
  assert(calls[1][1].includes('/git/tags/'));
  assert(calls.at(-1).includes('--prerelease'));
});

test('publication rejects PR/manual contexts and moved remote tags', (t) => {
  const root = fixture(t);
  for (const event of ['pull_request', 'workflow_dispatch']) {
    assert.throws(() => draft(root, { ...releaseEnv(), GITHUB_EVENT_NAME: event }, () => assert.fail('No API call allowed')));
  }
  assert.throws(() => draft(root, releaseEnv(), (_, args) => {
    assert.equal(args[0], 'api');
    return JSON.stringify({ object: { type: 'commit', sha: 'b'.repeat(40) } });
  }), /Remote tag moved/);
});

test('publication errors propagate; no automatic overwrite/retry', (t) => {
  const root = fixture(t);
  assemble(root, downloaded(root), join(root, 'release-assets'), revision, tag);
  assert.throws(() => draft(root, releaseEnv(), (_, args) => {
    if (args[0] === 'release') throw new Error('release already exists');
    return JSON.stringify({ object: { type: 'commit', sha: revision } });
  }), /release already exists/);
});
