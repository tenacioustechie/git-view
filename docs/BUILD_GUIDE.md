# Build and installation guide

Reviewed 11 October 2026 for Git View 0.1.0. The project pins Node 22,
pnpm 9.12.0, and Rust 1.98.1. The current tested release packages were built
from source checkpoint `d9af4e6`; exact results and checksums are retained in
[Build status](BUILD_STATUS.md) and `artifacts/manifest.json`.

## Choose how to run Git View

- Use a package from `artifacts/` to install the latest locally verified build.
- Build a release package from the current checkout to include local source
  changes.
- Use `pnpm tauri dev` while developing for automatic frontend rebuilding and a
  native application window.

Git View requires the `git` command at runtime and uses the operating system
webview. Configure Git identity, credentials, SSH, hooks, and signing outside
the application.

## Install the current verified package

### macOS Apple silicon

The current package is
`artifacts/Git-View_0.1.0_macos-arm64.zip`.

1. Extract the archive.
2. Move **Git View.app** to `/Applications`, or keep it elsewhere for testing.
3. Open **Git View.app**.

The package is unsigned and has not been notarized. macOS may require approval
through its first-launch security prompt. Intel Mac execution has not been
verified.

### Windows x64

Run `artifacts/Git-View_0.1.0_windows-x64-setup.exe`, complete the installer,
and launch **Git View** from the installed shortcut or application location.

The installer is unsigned, so Windows may report an unrecognized publisher.
Windows on ARM has not been verified.

The expected SHA-256 values and sizes for both packages are listed in
[Build status](BUILD_STATUS.md). To inspect the local files directly:

```sh
shasum -a 256 artifacts/Git-View_0.1.0_macos-arm64.zip
```

```powershell
Get-FileHash .\artifacts\Git-View_0.1.0_windows-x64-setup.exe -Algorithm SHA256
```

## Development prerequisites

Install the platform tools described in
[Development machine setup](../Dev-Machine-Setup.md). The required project
versions are:

| Tool | Required version or configuration |
| --- | --- |
| Node.js | `>=22.22.0 <23` |
| pnpm | `9.12.0` |
| Rust | `1.98.1`, selected automatically by `rust-toolchain.toml` |
| Rust components | `rustfmt`, `clippy` |
| Git | A working command-line installation |
| macOS native tools | Xcode Command Line Tools |
| Windows native tools | MSVC x64 C++ build tools and a Windows SDK |
| Windows webview | Microsoft Edge WebView2 Runtime |

Check the main versions before building:

```sh
node --version
pnpm --version
rustc --version
cargo --version
git --version
```

On Windows, use `pnpm.cmd` if PowerShell selects a blocked `.ps1` command shim.

## Install project dependencies

Run from the repository root:

```sh
pnpm install --frozen-lockfile
```

The first installation and native build require access to the package
registries and any platform packaging downloads. Dependencies and native
packages must be installed separately on each operating system; do not copy
`node_modules` or Rust build output between platforms.

## Run a development build

```sh
pnpm tauri dev
```

This starts Vite through Tauri and opens the native application. Stop it with
Ctrl-C in the terminal.

## Run the full source checks

These checks cover the frontend and Rust application before packaging:

```sh
pnpm typecheck
pnpm lint
pnpm test

cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml \
  --locked --all-targets -- -D warnings
```

Run the commands on each target operating system whose package will be shipped.
Most Git mutation tests create disposable repositories; do not substitute a
personal repository as a test fixture.

## Build, test, and install on macOS

The macOS helper builds from the current checkout and prints the resulting app
path:

```sh
scripts/macos-build.sh
```

For a quick repeat build that uses the existing installed dependencies and then
opens the new app:

```sh
scripts/macos-build.sh --skip-deps --open
```

Run all frontend and Rust checks plus the packaged release smoke check, replace
the existing application in `/Applications`, and open it with:

```sh
scripts/macos-build.sh --check --install --open
```

Quit a running Git View instance before using `--install`. The install step
replaces `/Applications/Git View.app` only after the source build and requested
checks succeed. It does not use `sudo`; if `/Applications` is not writable, the
script leaves the built application in place and asks you to copy it with
Finder. Run `scripts/macos-build.sh --help` for all options.

The equivalent direct command to build the Apple application bundle is:

```sh
pnpm tauri build --bundles app
```

The output is:

```text
src-tauri/target/release/bundle/macos/Git View.app
```

When the helper is given `--check`, it runs this packaged command-line and
startup check after building:

```sh
python3 scripts/release-smoke.py \
  'src-tauri/target/release/bundle/macos/Git View.app/Contents/MacOS/git-view'
```

The helper's `--open` option launches the application. The equivalent direct
command is:

```sh
open 'src-tauri/target/release/bundle/macos/Git View.app'
```

Without `--install`, the helper leaves any existing `/Applications/Git View.app`
unchanged. The normal build does not sign or notarize the application.

## Build, test, and install on Windows

The Windows helper builds from the current checkout and prints the resulting
executable, NSIS installer, and installer SHA-256:

```powershell
.\scripts\windows-build.ps1
```

For a quick repeat build that uses the existing installed dependencies and then
opens the new executable:

```powershell
.\scripts\windows-build.ps1 -SkipDeps -Launch
```

Run all frontend and Rust checks plus a release CLI/native-window smoke check,
silently install for the current user, and open the installed app with:

```powershell
.\scripts\windows-build.ps1 -Check -Install -Launch
```

Quit Git View before running the helper. Windows locks running executables, and
an existing primary instance would also invalidate the native-window smoke
check. `-Install` uses the generated NSIS installer and installs to
`%LOCALAPPDATA%\Git View` without requesting administrator access. Without
`-Install`, an existing installed application is unchanged. Run
`.\scripts\windows-build.ps1 -Help` for all options.

If local PowerShell policy blocks repository scripts, run the same command for
that process without changing machine policy:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows-build.ps1 -Check -Install -Launch
```

The equivalent direct command to build the NSIS installer is:

```powershell
pnpm.cmd tauri build --bundles nsis
```

The installer is written below:

```text
src-tauri\target\release\bundle\nsis\Git View_0.1.0_x64-setup.exe
```

When the helper is given `-Check`, it checks the release CLI and launches the
built executable against a disposable repository, then requires a visible
native window titled **Git View**. The direct CLI checks are:

```powershell
.\src-tauri\target\release\git-view.exe --help
.\src-tauri\target\release\git-view.exe --version
```

The repository's configured Windows verification machine also has the deeper
native and installed-package checks below:

```powershell
.\scripts\windows-verify.ps1
.\scripts\windows-release.ps1
.\scripts\windows-installer-smoke.ps1
```

Those scripts assume the checkout is `C:\codeme\git-view` and use the tool
locations configured for that machine. `windows-verify.ps1` runs frontend and
Rust checks plus the native desktop smoke suite. `windows-release.ps1` creates
the NSIS package and checks release CLI output. `windows-installer-smoke.ps1`
performs a silent test installation and verifies the installed executable and
native window in an interactive Windows desktop session.

## Run a built application from the checkout

The local wrappers select a release executable when one exists and otherwise
use a debug executable. They preserve the terminal's invoking directory, so
relative repository and project paths work as expected.

macOS or Linux:

```sh
scripts/git-view .
scripts/git-view repo ~/code/api
scripts/git-view group ./work.git-view.json
scripts/git-view group --scan ~/code --depth 3
```

Windows:

```powershell
.\scripts\git-view.cmd repo C:\codeme\api
.\scripts\git-view.cmd group C:\codeme\work.git-view.json
.\scripts\git-view.cmd group --scan C:\codeme --depth 3
```

The wrappers are local to the checkout and are not installed on `PATH`. Rebuild
after changing source code so a wrapper does not start an older executable.

## Current GitHub Actions pipeline

The repository has one workflow,
[`.github/workflows/build.yml`](../.github/workflows/build.yml), named
**Build, test and package**.

### How it starts

The workflow runs for pull requests, pushes to `main`, version tags matching
`v*`, and manual `workflow_dispatch` runs. Non-tag runs use read-only
`contents: read` permission. Concurrent non-tag runs for the same branch or pull
request cancel the older run; tag builds are never canceled this way.

To start it manually, open the repository's **Actions** tab, select **Build,
test and package**, choose **Run workflow**, select the branch, and start the run.

### Build matrix

One `verify` job runs independently on:

- `macos-15` for Apple silicon
- `macos-15-intel` for Intel x64
- `windows-2022` for x64
- `ubuntu-24.04`

The matrix has `fail-fast: false`, allowing the other operating systems to
continue if one fails.

### Steps on every runner

For each operating system, the workflow:

1. Checks out the source with a commit-pinned `actions/checkout` and does not
   retain repository credentials.
2. Installs the package-manager version declared in `package.json` with a
   commit-pinned `pnpm/action-setup`.
3. Installs Node 22.22.0 and enables the pnpm cache with a commit-pinned
   `actions/setup-node`.
4. Installs Rust 1.98.1 with rustfmt and Clippy.
5. Installs WebKitGTK and native packaging dependencies on Ubuntu.
6. Verifies that the package, Cargo, lockfile, and Tauri versions agree, then
   tests the release-assembly helpers.
7. Runs `pnpm install --frozen-lockfile`.
8. Runs TypeScript checking, ESLint, frontend tests, and the production frontend
   build.
9. Runs Rust formatting, strict all-target Clippy with the native-smoke feature,
   and the locked Rust test suite.
   If Windows cannot load the Rust library-test executable, the job captures its
   PE headers, imports, resolved direct DLL versions, embedded manifest, and a
   direct loader probe as `git-view-windows-loader-diagnostics`, then remains
   failed.
10. Builds one package set for that matrix entry: arm64 or x64 DMG, x64 NSIS,
    or x64 DEB and AppImage. macOS packages use ad-hoc signing; packages are not
    notarized. Windows and Linux packages are unsigned.
11. Runs bounded CLI checks against the release executable, copies only the
    expected package formats, and records source revision, size, and SHA-256 in
    a platform manifest.
12. Uploads the normalized packages and manifest as `git-view-macos-arm64`,
    `git-view-macos-x64`, `git-view-windows-x64`, or `git-view-linux-x64` for
    14 days.

The verification commands in the workflow are currently:

```sh
node scripts/ci-release.mjs check
node --test scripts/ci-release.test.mjs
pnpm install --frozen-lockfile
pnpm typecheck
pnpm lint
pnpm test -- --maxWorkers=2
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets \
  --features native-smoke -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

The platform-specific Tauri command follows those checks and invokes the
configured `pnpm build` before-build command again. After a job finishes, its
package and source manifest can be downloaded from the run's **Artifacts**
section.

### Version-tag draft release

A push of a tag matching `v*` must also match the version in `package.json`,
`src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, and `src-tauri/tauri.conf.json`.
After all four matrix jobs pass, a separate job downloads exactly those four
artifacts, rejects missing, extra, renamed, empty, symlinked, revision-mismatched,
or checksum-mismatched inputs, and writes `SHA256SUMS.txt`.

The job then creates a new **draft** GitHub Release for the existing tag. It
verifies that the remote tag still resolves to the tested commit and refuses to
overwrite an existing release. The generated notes call out unsigned packages,
platform limits, and the need for downloaded-package acceptance before a person
publishes the draft. Pull requests, `main` pushes, and manual runs cannot create
a release and do not receive `contents: write` permission.

### What the workflow does not currently do

- It does not run `scripts/native-smoke.py` or the interactive Windows native
  desktop and installed-package checks.
- It does not sign or notarize packages.
- It does not publish a draft GitHub Release; publication remains a reviewed
  manual action.
- A hosted run has not yet been recorded as verified in Build status.

The uploaded Actions artifacts and tag-triggered release drafts are build outputs
for inspection. Local macOS arm64 and Windows 11 x64 results remain the current
platform acceptance evidence until hosted jobs and downloaded packages are
separately recorded in [Build status](BUILD_STATUS.md).

## Troubleshooting

| Problem | Check |
| --- | --- |
| `pnpm install --frozen-lockfile` rejects the lockfile | Use pnpm 9.12.0 and confirm `package.json` and `pnpm-lock.yaml` come from the same revision. |
| Rust selects a different compiler | Run `rustup show`; the repository should select Rust 1.98.1 from `rust-toolchain.toml`. |
| macOS cannot find a compiler or linker | Install Xcode Command Line Tools and check `xcode-select -p`. |
| Windows reports that `link.exe` is missing | Add the Visual Studio Desktop development with C++ workload and a Windows SDK. |
| The native window is blank or does not open on Windows | Confirm that the Edge WebView2 Runtime is installed. |
| Git View cannot perform Git operations | Confirm `git --version` works in the environment used to launch the app. |
| A wrapper starts old behavior | Rebuild the application or remove the stale debug/release executable before rebuilding. |

See [User guide](USER_GUIDE.md) for application workflows and
[Acceptance](ACCEPTANCE.md) for the broader manual and automated acceptance
expectations.
