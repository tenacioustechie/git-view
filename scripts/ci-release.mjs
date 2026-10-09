// GitHub packaging helpers. No dependencies, credentials, or publication on import.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync, spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const packages = {
  'macos-arm64': [['dmg', '.dmg']],
  'macos-x64': [['dmg', '.dmg']],
  'windows-x64': [['nsis', '-setup.exe']],
  'linux-x64': [['deb', '.deb'], ['appimage', '.AppImage']],
};
const read = (file) => readFileSync(file, 'utf8');
const json = (file) => JSON.parse(read(file));
const digest = (file) => createHash('sha256').update(readFileSync(file)).digest('hex');
const saveJson = (file, value) => writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`, { flag: 'wx' });
const manifestName = (platform) => `manifest-${platform}.json`;
const assetName = (version, platform, suffix) => `Git-View_${version}_${platform}${suffix}`;

export function checkVersion(root, tag = '') {
  const version = json(join(root, 'package.json')).version;
  // Release names deliberately exclude build metadata and unsafe path characters.
  assert.match(version, /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/);
  const cargo = read(join(root, 'src-tauri/Cargo.toml')).match(/\[package\]([\s\S]*?)(?:\n\[|$)/)?.[1];
  const cargoVersion = cargo?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  const lockVersion = read(join(root, 'src-tauri/Cargo.lock')).match(/\[\[package\]\]\s+name = "git-view"\s+version = "([^"]+)"/)?.[1];
  assert.equal(json(join(root, 'src-tauri/tauri.conf.json')).version, version, 'Tauri version differs');
  assert.equal(cargoVersion, version, 'Cargo package version differs');
  assert.equal(lockVersion, version, 'Cargo lockfile package version differs');
  if (tag) assert.equal(tag, `v${version}`, 'Tag must exactly match all app versions');
  return version;
}

function freshDirectory(directory) {
  if (existsSync(directory)) assert.equal(readdirSync(directory).length, 0, `Refusing nonempty output: ${directory}`);
  mkdirSync(directory, { recursive: true });
}

function regularFile(file) {
  assert(lstatSync(file).isFile(), `Expected a regular file, not a symlink/directory: ${file}`);
  assert(lstatSync(file).size > 0, `Empty package: ${file}`);
}

export function collect(root, platform, revision, output = join(root, 'release-assets')) {
  assert(Object.hasOwn(packages, platform), 'Unknown package platform');
  assert.match(revision, /^[0-9a-f]{40}$/);
  const version = checkVersion(root);
  const sources = packages[platform].map(([folder, suffix]) => {
    const directory = join(root, 'src-tauri/target/release/bundle', folder);
    const matches = readdirSync(directory).filter((name) => name.endsWith(suffix));
    assert.equal(matches.length, 1, `Expected exactly one ${folder} package`);
    const file = join(directory, matches[0]);
    regularFile(file);
    return { file, name: assetName(version, platform, suffix) };
  });
  freshDirectory(output);
  const files = sources.map(({ file, name }) => {
    copyFileSync(file, join(output, name));
    return { name, bytes: lstatSync(file).size, sha256: digest(file) };
  });
  const manifest = { version, revision, platform, signing: platform.startsWith('macos-') ? 'ad-hoc; not notarized' : 'unsigned', files };
  saveJson(join(output, manifestName(platform)), manifest);
  return manifest;
}

export function assemble(root, input, output, revision, tag) {
  assert.match(revision, /^[0-9a-f]{40}$/);
  assert(tag, 'A release tag is required');
  const version = checkVersion(root, tag);
  assert.deepEqual(readdirSync(input).sort(), Object.keys(packages).map((p) => `git-view-${p}`).sort(), 'Missing or unexpected platform artifacts');
  const verified = [];
  for (const [platform, formats] of Object.entries(packages)) {
    const directory = join(input, `git-view-${platform}`);
    assert(lstatSync(directory).isDirectory(), 'Artifact directory cannot be a symlink');
    const metadata = manifestName(platform);
    regularFile(join(directory, metadata));
    const manifest = json(join(directory, metadata));
    assert.equal(manifest.version, version);
    assert.equal(manifest.revision, revision, 'Package source revision differs');
    assert.equal(manifest.platform, platform);
    const expected = formats.map(([, suffix]) => assetName(version, platform, suffix)).sort();
    assert.deepEqual(manifest.files.map((f) => f.name).sort(), expected, 'Wrong or duplicate package names');
    assert.deepEqual(readdirSync(directory).sort(), [...expected, metadata].sort(), 'Unexpected artifact files');
    for (const entry of manifest.files) {
      const file = join(directory, entry.name);
      regularFile(file);
      assert.equal(lstatSync(file).size, entry.bytes, 'Package size differs');
      assert.equal(digest(file), entry.sha256, 'Package checksum differs');
      verified.push([file, entry.name]);
    }
    verified.push([join(directory, metadata), metadata]);
  }
  freshDirectory(output);
  const checksums = verified.map(([file, name]) => {
    copyFileSync(file, join(output, name));
    return `${digest(file)}  ${name}`;
  });
  writeFileSync(join(output, 'SHA256SUMS.txt'), `${checksums.sort().join('\n')}\n`, { flag: 'wx' });
  return version;
}

export function smoke(root, binary) {
  const version = checkVersion(root);
  const temporary = mkdtempSync(join(tmpdir(), 'git-view-cli-'));
  try {
    for (const [args, status, expected] of [
      [['--help'], 0, 'Usage:'], [['--version'], 0, `Git View ${version}`],
      [['repo', 'missing'], 2, ''], [['group', '--scan', 'missing'], 2, ''],
      [['group', '--scan', '.', '--depth', '11'], 2, ''],
    ]) {
      const result = spawnSync(binary, args, { cwd: temporary, encoding: 'utf8', timeout: 15_000 });
      assert.ifError(result.error);
      assert.equal(result.status, status, `Release CLI ${args}: ${result.stderr}`);
      assert(result.stdout.includes(expected), `Unexpected release CLI output: ${result.stdout}`);
    }
  } finally {
    rmSync(temporary, { recursive: true }); // Only this invocation's disposable CLI fixture.
  }
}

export function releaseBinary(root, platform = process.platform) {
  return join(root, `src-tauri/target/release/git-view${platform === 'win32' ? '.exe' : ''}`);
}

export function draft(root, env, run = execFileSync) {
  assert.equal(env.GITHUB_EVENT_NAME, 'push');
  assert.equal(env.GITHUB_REF_TYPE, 'tag');
  const version = checkVersion(root, env.GITHUB_REF_NAME);
  assert.equal(env.GITHUB_REF_NAME, `v${version}`);
  assert.match(env.GITHUB_SHA, /^[0-9a-f]{40}$/);
  const api = (endpoint) => JSON.parse(run('gh', ['api', endpoint], { encoding: 'utf8' }));
  let target = api(`repos/{owner}/{repo}/git/ref/tags/${env.GITHUB_REF_NAME}`).object;
  for (let depth = 0; target.type === 'tag' && depth < 5; depth++) {
    assert.match(target.sha, /^[0-9a-f]{40}$/);
    target = api(`repos/{owner}/{repo}/git/tags/${target.sha}`).object;
  }
  assert.equal(target.type, 'commit', 'Release tag does not resolve to a commit');
  assert.equal(target.sha, env.GITHUB_SHA, 'Remote tag moved since the tested build');
  const notes = join(root, 'RELEASE_NOTES.md');
  writeFileSync(notes, `# Git View ${version}\n\nSource: ${env.GITHUB_SHA}\n\n` +
    'Download the DMG for your Mac (arm64 = Apple Silicon, x64 = Intel), the Windows x64 setup EXE, or the Linux x64 DEB/AppImage. SHA256SUMS.txt covers packages and source manifests.\n\n' +
    'Install Git separately and configure your identity/credentials. Windows requires WebView2 (the installer can download it). Linux packages are built on Ubuntu 24.04; older distributions are not supported by these binaries. Linux stash deletion is unavailable.\n\n' +
    'macOS packages are ad-hoc signed, not Developer ID signed or notarized. Windows/Linux packages are unsigned. OS security policy may warn or block installation; do not disable system protection. Checksums establish file integrity, not publisher identity.\n\n' +
    'These packages passed source tests and release CLI checks, not full hosted GUI/installer acceptance. Maintainer: test downloaded packages on each target OS, review known limitations and update these notes before publishing this draft.\n', { flag: 'wx' });
  const assets = readdirSync(join(root, 'release-assets')).sort().map((name) => join(root, 'release-assets', name));
  run('gh', ['release', 'create', env.GITHUB_REF_NAME, ...assets, '--verify-tag', '--draft',
    '--title', `Git View ${version}`, '--notes-file', notes, ...(version.includes('-') ? ['--prerelease'] : [])], { stdio: 'inherit' });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = process.cwd();
  const tag = process.env.GITHUB_REF_TYPE === 'tag' ? process.env.GITHUB_REF_NAME : '';
  const revision = process.env.GITHUB_SHA;
  switch (process.argv[2]) {
    case 'check': console.log(`Validated Git View ${checkVersion(root, tag)}`); break;
    case 'collect': collect(root, process.argv[3], revision); break;
    case 'assemble': assemble(root, join(root, 'downloaded-packages'), join(root, 'release-assets'), revision, tag); break;
    case 'smoke': smoke(root, releaseBinary(root)); break;
    case 'draft': draft(root, process.env); break;
    default: throw new Error('Usage: node scripts/ci-release.mjs check|collect PLATFORM|assemble|smoke|draft');
  }
}
