# Development machine setup

Prepare a Mac for local Git View development and a Windows machine for native builds and remote testing over SSH.

Updated: 19 September 2026. This guide installs prerequisites; application build commands become available after the planned Tauri project is scaffolded. The commands below have been checked against the linked documentation, but have not been executed on your Windows machine.

## 1. Versions and machine roles

| Component | Mac | Windows |
| --- | --- | --- |
| Operating system | Your current Mac; build for its native architecture first | Windows 11 x64 is the initial test target |
| Native compiler tools | Xcode Command Line Tools | Visual Studio Build Tools, C++ workload, Windows SDK |
| Rust | Stable toolchain managed by rustup | Stable `x86_64-pc-windows-msvc` toolchain managed by rustup |
| JavaScript | Node 22, matching the current development environment | Node 22 x64 |
| Package manager | pnpm 9.12.0 initially | pnpm 9.12.0 initially |
| Git | Installed command-line Git | Git for Windows, available in PowerShell and SSH |
| Webview | Provided by macOS | Microsoft Edge WebView2 Runtime |
| Remote access | Built-in SSH client | Windows OpenSSH Server |

Node 22 and pnpm 9.12.0 are the provisional project baseline, chosen to match the existing Mac. Once the project has `package.json`, a lockfile, and a Rust toolchain pin, follow those recorded versions. Use the same versions on both machines and in CI. Do not install a second Node version manager just for this guide.

During planning, the Mac already had Git 2.51.2, Node 22.22.0, pnpm 9.12.0, and Xcode Command Line Tools. Rust was missing. Recheck before installing anything already present.

## 2. macOS setup

Run these commands in Terminal with your normal user account. Complete installer dialogs when prompted.

### 2.1 Xcode Command Line Tools

Check the selected tools:

```sh
xcode-select -p
xcrun --find clang
```

If they are missing:

```sh
xcode-select --install
```

Wait for installation to finish and repeat the checks. Full Xcode is not required for this desktop-only project. See [Tauri's macOS prerequisites](https://v2.tauri.app/start/prerequisites/#macos).

### 2.2 Homebrew

```sh
brew --version
```

If Homebrew is absent, follow the installer and shell setup shown on the [official Homebrew site](https://brew.sh/). Its final instructions account for the different installation locations on Apple silicon and Intel Macs. Open a new Terminal and check `brew --version` again.

### 2.3 Git

```sh
git --version
```

An existing working Git installation is sufficient. To install Git through Homebrew if needed:

```sh
brew install git
```

### 2.4 Node and pnpm

First check the existing environment:

```sh
node --version
npm --version
pnpm --version
```

If your existing version manager already provides Node 22, keep using it. Otherwise, this Homebrew route installs that major version:

```sh
brew install node@22
export PATH="$(brew --prefix node@22)/bin:$PATH"
```

For that Homebrew route only, add the same `export PATH=...` line to `~/.zshrc` so new terminals find it. See [Homebrew's Node 22 formula](https://formulae.brew.sh/formula/node@22).

If pnpm is absent or a different version is active, install the agreed baseline:

```sh
npm install --global pnpm@9.12.0
pnpm --version
```

Use your existing user-owned Node installation; if a global install reports a permissions error, fix that installation's npm prefix rather than using `sudo npm`. The project may later pin pnpm through its `packageManager` field. See [pnpm installation](https://pnpm.io/installation).

### 2.5 Rust through Homebrew

Use the rustup formula so the project can pin a compiler version and add targets:

```sh
brew install rustup
export PATH="$(brew --prefix rustup)/bin:$HOME/.cargo/bin:$PATH"
rustup default stable
rustup component add rustfmt clippy
```

Add this line to `~/.zshrc`, after Homebrew is made available:

```sh
export PATH="$(brew --prefix rustup)/bin:$HOME/.cargo/bin:$PATH"
```

Open a new Terminal, then check:

```sh
rustup show
rustc --version
cargo --version
cargo fmt --version
cargo clippy --version
```

The current Homebrew formula provides `rustup` directly; it no longer provides `rustup-init`. Do not also install the separate `rust` formula for this setup. If Rust was already installed another way, check `which -a rustup rustc cargo` and use one toolchain manager consistently. See [Homebrew rustup](https://formulae.brew.sh/formula/rustup).

### 2.6 Verify native compilation

This small test compiles and runs a Rust executable without downloading project dependencies. It also checks the native linker:

```sh
gitview_probe_dir=$(mktemp -d /tmp/git-view-rust-check.XXXXXX)
printf '%s\n' 'fn main() { println!("Git View Rust toolchain OK"); }' > "$gitview_probe_dir/main.rs"
rustc "$gitview_probe_dir/main.rs" -o "$gitview_probe_dir/git-view-rust-check"
"$gitview_probe_dir/git-view-rust-check"
```

Expected output: `Git View Rust toolchain OK`. The test files remain in the printed/assigned temporary directory and can be removed after inspection.

## 3. Windows setup

These instructions target native Windows, not WSL. The first test machine should be Windows 11 x64; Windows on ARM needs a separate architecture/toolchain choice.

Choose the Windows account that will perform builds and accept SSH connections. A standard local account such as `gitbuild` is a useful dedicated choice. Create it through Windows Settings → Accounts → Other users if needed, and sign in once to create its profile. Run per-user installs, especially Rust and pnpm, as that account. Use an administrator account only for system installers and the steps explicitly marked Administrator.

### 3.1 Visual Studio Build Tools

Download **Build Tools for Visual Studio** from [Microsoft's downloads page](https://visualstudio.microsoft.com/downloads/) and run the installer. A full Visual Studio IDE installation with the same workload is also sufficient.

Select **Desktop development with C++**, including:

- MSVC C++ tools for x64/x86.
- A current Windows SDK supported by the installer.
- The workload's recommended components.

Finish installation and any requested restart before installing/testing Rust. See [Rust's Windows installation guidance](https://rust-lang.github.io/rustup/installation/windows.html).

### 3.2 Git for Windows

Install using the [official Git for Windows download](https://git-scm.com/install/windows). Choose the installer option that makes Git available from the command line and third-party applications.

Open a new **ordinary PowerShell window as the build account**:

```powershell
git --version
Get-Command git
```

### 3.3 Node and pnpm

Use [Node's official download page](https://nodejs.org/en/download) to select Node 22, Windows, x64, and the MSI installer, matching the current project baseline. Keep the npm and PATH options enabled. If Node 22 is already supplied by your existing version manager, use that installation instead.

Open a new ordinary PowerShell window as the build account:

```powershell
node --version
npm.cmd --version
npm.cmd install --global pnpm@9.12.0
pnpm.cmd --version
```

The `.cmd` suffix avoids PowerShell selecting an npm/pnpm `.ps1` shim that execution policy may block. There is no need to weaken machine-wide PowerShell execution policy for these commands.

### 3.4 Rust and Cargo

Download and run the Windows x64 installer from [rustup](https://rustup.rs/) **as the build account**. Use the default MSVC installation. After installation, open another PowerShell window:

```powershell
rustup default stable-x86_64-pc-windows-msvc
rustup component add rustfmt clippy
rustup show
rustc --version
cargo --version
cargo fmt --version
cargo clippy --version
```

The active host/toolchain should use `x86_64-pc-windows-msvc`. Do not select a GNU/MinGW toolchain for this build. The normal per-user executable directory is `%USERPROFILE%\.cargo\bin`. See [Rust on Windows](https://rust-lang.github.io/rustup/installation/windows.html).

### 3.5 WebView2 Runtime and installer format

Windows 11 normally includes WebView2. If it is absent, use Microsoft's **Evergreen Runtime** installer from the [WebView2 download page](https://developer.microsoft.com/en-us/microsoft-edge/webview2/). An Edge browser installation by itself is not the readiness test; the eventual native app must launch successfully.

For the initial release, prefer the Windows `.exe` installer target. If we also build MSI packages, Tauri documents an additional **VBScript optional feature** prerequisite; enable it only if the selected MSI packaging path needs it. See [Tauri's Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows).

### 3.6 Check native compilation and create a workspace

In ordinary PowerShell as the build account:

```powershell
$gitViewProbeDir = Join-Path $env:TEMP ('git-view-rust-check-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $gitViewProbeDir | Out-Null
$gitViewProbeSource = Join-Path $gitViewProbeDir 'main.rs'
$gitViewProbeExe = Join-Path $gitViewProbeDir 'git-view-rust-check.exe'
Set-Content -Path $gitViewProbeSource -Encoding ascii -Value 'fn main() { println!("Git View Rust toolchain OK"); }'
rustc $gitViewProbeSource -o $gitViewProbeExe
if ($LASTEXITCODE -ne 0) { throw 'Rust compilation failed; check the C++ tools and Windows SDK.' }
& $gitViewProbeExe

$gitViewWorkspace = Join-Path $env:USERPROFILE 'source\git-view'
New-Item -ItemType Directory -Force -Path $gitViewWorkspace | Out-Null
$gitViewWorkspace
```

Expected output includes `Git View Rust toolchain OK` and the workspace path, for example `C:\Users\gitbuild\source\git-view`. The temporary test executable can be removed after inspection.

If compilation reports a missing linker, modify the Visual Studio Build Tools installation to include the C++ workload and SDK. A successful version command alone does not prove compilation works.

## 4. Enable Windows OpenSSH Server

Use a network route reachable from the Mac: the same trusted LAN or an existing private VPN. The examples use TCP port 22; no router port forwarding is needed for a LAN connection.

### 4.1 Install and start the service

Run **PowerShell as Administrator** on Windows:

```powershell
Get-WindowsCapability -Online -Name 'OpenSSH.Server*'
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
Set-Service -Name sshd -StartupType Automatic
Start-Service sshd
Get-Service sshd
```

If the capability is already installed, skip `Add-WindowsCapability`. If Windows requests a reboot, restart before continuing. The service should report `Running`. See [Microsoft's OpenSSH installation guide](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_install_firstuse).

### 4.2 Check the firewall and address

Still in Administrator PowerShell:

```powershell
Get-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -ErrorAction SilentlyContinue
Get-NetConnectionProfile
Get-NetIPConfiguration
```

Installation normally creates the SSH firewall rule. For a dedicated build machine, the following example enables access only from the Mac's address on a Private/Domain network. **Replace the example address with the Mac's LAN or VPN source address first.** On a shared SSH server, preserve any other required clients when choosing the rule's address scope.

```powershell
$gitViewMacAddress = '192.168.1.50'
if (Get-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -ErrorAction SilentlyContinue) {
    Set-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -Enabled True -Profile Private,Domain -RemoteAddress $gitViewMacAddress
} else {
    New-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -DisplayName 'OpenSSH Server (sshd)' -Enabled True -Direction Inbound -Protocol TCP -LocalPort 22 -Action Allow -Profile Private,Domain -RemoteAddress $gitViewMacAddress
}
```

If the trusted LAN is classified Public, change that connection to Private in Windows network settings or choose an appropriate rule for your approved network. Keep the firewall enabled. Record the Windows address or resolvable hostname for the Mac's SSH configuration; a DHCP reservation helps keep it stable.

### 4.3 Use PowerShell for remote commands

The following is part of this guide's setup: later SSH examples assume PowerShell, while Windows OpenSSH initially defaults to `cmd.exe`. In **Administrator PowerShell**:

```powershell
$gitViewDefaultShell = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
New-ItemProperty -Path 'HKLM:\SOFTWARE\OpenSSH' -Name DefaultShell -Value $gitViewDefaultShell -PropertyType String -Force
Restart-Service sshd
```

This changes the SSH shell for the machine. Use a dedicated build machine or account for predictable automation. See [Microsoft's default-shell configuration](https://learn.microsoft.com/en-us/windows-server/administration/OpenSSH/openssh-server-configuration#configuring-the-default-shell-for-openssh-in-windows).

## 5. Configure SSH keys from the Mac

### 5.1 Create a dedicated key on the Mac

Run in Mac Terminal. Choose another filename if this one already exists; do not overwrite an existing key:

```sh
mkdir -p ~/.ssh
chmod 700 ~/.ssh
ssh-keygen -t ed25519 -f ~/.ssh/git_view_windows -C git-view-windows
ssh-add ~/.ssh/git_view_windows
cat ~/.ssh/git_view_windows.pub
```

Choose a passphrase and load the key into the Mac's agent with `ssh-add` before the automated build session. Copy the **single public-key line** printed by the last command. The private key stays on the Mac. Recheck agent availability after a restart or a new login.

### 5.2 Install the public key for a standard Windows account

In ordinary PowerShell, **signed in as the Windows build account**, replace the placeholder with the complete public-key line:

```powershell
$gitViewPublicKey = 'PASTE_THE_COMPLETE_ssh-ed25519_PUBLIC_KEY_LINE_HERE'
if (-not $gitViewPublicKey.StartsWith('ssh-ed25519 ')) { throw 'Paste the public key before continuing.' }
$gitViewSshDir = Join-Path $env:USERPROFILE '.ssh'
$gitViewAuthorizedKeys = Join-Path $gitViewSshDir 'authorized_keys'
New-Item -ItemType Directory -Force -Path $gitViewSshDir | Out-Null
Add-Content -Path $gitViewAuthorizedKeys -Encoding ascii -Value $gitViewPublicKey
```

This appends the key and preserves existing keys. The filename is `authorized_keys`, with no `.txt` extension. It belongs to the build account's profile, not the administrator account used to install Windows features. See [Microsoft's standard-user key setup](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_keymanagement#standard-user).

### 5.3 Alternative: the build account is an administrator

Use this section **instead of 5.2** if the build account belongs to the local Administrators group under the default SSH configuration. In **Administrator PowerShell**, paste the same public key:

```powershell
$gitViewPublicKey = 'PASTE_THE_COMPLETE_ssh-ed25519_PUBLIC_KEY_LINE_HERE'
if (-not $gitViewPublicKey.StartsWith('ssh-ed25519 ')) { throw 'Paste the public key before continuing.' }
$gitViewAdminKeys = Join-Path $env:ProgramData 'ssh\administrators_authorized_keys'
Add-Content -Path $gitViewAdminKeys -Encoding ascii -Value $gitViewPublicKey
icacls.exe $gitViewAdminKeys /inheritance:r /grant:r '*S-1-5-32-544:F' '*S-1-5-18:F'
icacls.exe $gitViewAdminKeys
```

The final permissions should contain only Administrators and SYSTEM; remove any other explicit entries if the file already existed. The SIDs avoid translated Windows group names. This administrator key file is shared by administrator accounts; a dedicated standard build account gives a narrower account boundary. See [Microsoft's administrator key setup](https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_keymanagement#administrative-user).

### 5.4 Add a Mac SSH alias and verify the host

Add the following block to `~/.ssh/config`, preferably before a broad `Host *` block. Replace the Windows address and username:

```sshconfig
Host git-view-windows
    HostName 192.168.1.100
    User gitbuild
    Port 22
    IdentityFile ~/.ssh/git_view_windows
    IdentitiesOnly yes
    AddKeysToAgent yes
    ServerAliveInterval 30
    ServerAliveCountMax 3
```

Then on the Mac:

```sh
chmod 600 ~/.ssh/config
ssh git-view-windows
```

On first connection, compare the offered host fingerprint with the matching public host key on Windows. For an Ed25519 host key, display it in **Administrator PowerShell**:

```powershell
& "$env:WINDIR\System32\OpenSSH\ssh-keygen.exe" -lf "$env:ProgramData\ssh\ssh_host_ed25519_key.pub"
```

Accept the matching host key and confirm the remote prompt belongs to the intended account with `whoami`. Type `exit` to return to the Mac. Keep normal host-key checking enabled.

### 5.5 Verify unattended commands and PATH

From Mac Terminal, after the first host-key confirmation and after loading the key into the agent:

```sh
ssh -o BatchMode=yes git-view-windows 'whoami'
ssh -o BatchMode=yes git-view-windows 'git --version'
ssh -o BatchMode=yes git-view-windows 'node --version'
ssh -o BatchMode=yes git-view-windows 'pnpm.cmd --version'
ssh -o BatchMode=yes git-view-windows 'rustc --version'
ssh -o BatchMode=yes git-view-windows 'cargo --version'
ssh -o BatchMode=yes git-view-windows 'rustup show active-toolchain'
```

Every command should succeed without asking for a password/passphrase. Also rerun the Windows Rust compilation probe from section 3.6 inside an SSH session. That proves the remote account can find and use the linker, not just run `cargo --version`.

If local PowerShell works but SSH cannot find tools, inspect the remote paths:

```sh
ssh git-view-windows 'Get-Command git,node,pnpm.cmd,rustc,cargo | Select-Object Name,Source'
ssh git-view-windows '$env:Path'
```

Ensure the build account's persistent User PATH contains `%USERPROFILE%\.cargo\bin` and the npm global command directory (normally `%APPDATA%\npm`), and that Git/Node are in the appropriate persistent PATH. Do not rely solely on a terminal profile or version manager that only initializes interactively. After changing environment settings, restart `sshd` from a local Administrator PowerShell and open a new SSH session; restart Windows if the service still sees stale machine environment values. Repeat the unattended checks.

### 5.6 Verify file transfer

On the Mac, create a small probe file and copy it to the remote account's home directory:

```sh
gitview_ssh_probe=$(mktemp /tmp/git-view-ssh-probe.XXXXXX)
printf '%s\n' 'Git View transfer OK' > "$gitview_ssh_probe"
scp "$gitview_ssh_probe" git-view-windows:git-view-ssh-probe.txt
ssh -o BatchMode=yes git-view-windows 'Get-Content (Join-Path $env:USERPROFILE "git-view-ssh-probe.txt")'
```

Expected output: `Git View transfer OK`. The probe files can be removed afterward. We can transfer source through SSH/SFTP, or clone a repository once its remote and access are configured. Keep Windows `node_modules` and Rust build outputs separate from Mac outputs; install dependencies natively on each machine.

## 6. Keep the machines ready for a build session

Plug both machines into power, keep their network connection available, and prevent system sleep for the build window. On the Mac, a temporary Terminal session can do this:

```sh
caffeinate -i
```

Leave that command running and stop it with Ctrl-C afterward. On Windows, use Settings → System → Power to prevent sleep while plugged in for the build window, then restore your usual setting afterward. Reboots and pending installer prompts should be resolved before unattended work starts.

SSH is enough for compilation, Git integration tests, and artifact retrieval. Native GUI testing needs a separate desktop-capable test session. For manual testing, sign into the Windows desktop locally or through Remote Desktop and launch the built app. For automated visible UI tests, run the test worker in the interactive desktop session; a Windows service or SSH session alone does not establish that setup. Locking/disconnecting a desktop can affect those tests. See [Microsoft's UI testing guidance](https://learn.microsoft.com/en-us/azure/devops/pipelines/test/ui-testing-considerations?view=azure-devops).

## 7. Project build checks after scaffolding

The current Windows build checkout is **`C:\codeme\git-view`**, accessed through **`git-view-windows`**. This replaces the example per-user workspace path earlier in this guide. The machine mapping and synchronization policy are recorded in [the runbook](docs/OVERNIGHT.md#configured-development-machines).

No additional SSH setting is needed for the directory. For example, this command selects the repository explicitly:

```sh
ssh -o BatchMode=yes git-view-windows 'git -C C:/codeme/git-view status --short --branch'
```

For build commands, the Windows build script must set its working directory to `C:\codeme\git-view` before running pnpm/Cargo. Sharing the same Git remote does not transfer new edits automatically; the coordinator will synchronize source checkpoints before testing.

These commands are planned, not available in the documentation-only repository today. Run them from the project root once `package.json`, the lockfile, and `src-tauri/Cargo.toml` exist. On Windows use `pnpm.cmd` if needed.

```sh
pnpm install --frozen-lockfile
pnpm typecheck
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build
```

The first native build needs access to package registries and any platform packaging downloads. Run it once while someone is available to address installer/network prompts. Build checks operate in disposable test repositories, not your personal working repositories.

Verify the packaged app launches and can find installed Git. Windows and macOS artifacts must be built and checked separately. Application signing/notarization is not needed to establish the compiler setup; distribution credentials are a later packaging task. See [Tauri build and packaging](https://v2.tauri.app/distribute/).

## 8. Readiness checklist and handoff

- [ ] Mac: compiler tools, Git, Node, pnpm, Rust, rustfmt, and Clippy available in a new Terminal.
- [ ] Mac: Rust compilation probe succeeds.
- [ ] Windows: C++ tools, SDK, Git, Node, pnpm, MSVC Rust, and WebView2 available.
- [ ] Windows: Rust compilation probe succeeds as the intended build account.
- [ ] Windows: workspace exists and is writable by that account.
- [ ] SSH: service starts automatically and is reachable from the Mac.
- [ ] SSH: host fingerprint verified and public-key authentication works.
- [ ] SSH: all BatchMode version checks and remote compilation succeed.
- [ ] SSH: file transfer succeeds.
- [ ] Both machines: power/network/sleep settings support the build window.
- [ ] GUI testing: record whether a Windows desktop session is available or still needs setup.

Provide these details when ready; no passwords or private keys are needed in the conversation:

```text
Mac workspace: /Users/brian/codeme/git-view
Windows SSH alias: git-view-windows
Windows workspace: C:\codeme\git-view
Windows version / architecture: Windows 11 / x64
BatchMode checks: passed / failing command and error
Rust compilation: Mac passed, Windows local passed, Windows SSH passed
GUI desktop testing: available / not yet configured
Availability: machine stays awake until ...
```

## 9. Troubleshooting

| Symptom | Check |
| --- | --- |
| Mac cannot find `rustup` after Homebrew install | Add the rustup formula's `bin` to PATH and open a new Terminal |
| Mac cannot find the C compiler or linker | Finish Command Line Tools installation and check `xcode-select -p` |
| Windows cannot find `link.exe`, or Rust linking fails | C++ workload and Windows SDK are installed; rerun the compilation probe |
| PowerShell blocks `pnpm.ps1` / `npm.ps1` | Use `pnpm.cmd` / `npm.cmd` |
| SSH times out | Windows is awake, address is correct, VPN/LAN is reachable, firewall profile/source address matches |
| SSH connection refused | `sshd` is running and listening on the intended port |
| SSH says `Permission denied (publickey)` | Correct user, correct standard/admin key file, full public-key line, file permissions, key loaded on Mac |
| SSH works interactively but BatchMode fails | Load the key into the Mac's agent and establish verified host trust before the build |
| Rust works locally but fails over SSH | Toolchain belongs to the SSH account; persistent PATH is correct; service environment refreshed |
| App builds but its GUI fails to start | Verify WebView2 and test in a real desktop session; collect application logs |

For SSH authentication failures, inspect Windows Event Viewer → Applications and Services Logs → OpenSSH → Operational, and use `ssh -v git-view-windows` on the Mac to identify which key/account is being offered. Avoid publishing full diagnostic logs without checking them for private paths and account details.
