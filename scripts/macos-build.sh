#!/bin/sh
set -eu

usage() {
  cat <<'EOF'
Usage: scripts/macos-build.sh [options]

Build the unsigned macOS Git View application from the current checkout.

Options:
  --check       Run frontend and Rust checks, then test the packaged app.
  --install     Replace /Applications/Git View.app after a successful build.
  --open        Open the installed app, or the built app without --install.
  --skip-deps   Skip pnpm install --frozen-lockfile for a repeat build.
  -h, --help    Show this help.

Examples:
  scripts/macos-build.sh
  scripts/macos-build.sh --skip-deps --open
  scripts/macos-build.sh --check --install --open
EOF
}

run_checks=false
install_app=false
open_app=false
install_deps=true

while [ "$#" -gt 0 ]; do
  case "$1" in
    --check) run_checks=true ;;
    --install) install_app=true ;;
    --open) open_app=true ;;
    --skip-deps) install_deps=false ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if [ "$(uname -s)" != "Darwin" ]; then
  echo 'This script builds the macOS application and must run on macOS.' >&2
  exit 1
fi

git_view_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
app="$git_view_root/src-tauri/target/release/bundle/macos/Git View.app"
binary="$app/Contents/MacOS/git-view"
install_path='/Applications/Git View.app'

for command in pnpm cargo python3; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Required command is unavailable: $command" >&2
    exit 1
  fi
done

cd "$git_view_root"

if [ "$install_deps" = true ]; then
  pnpm install --frozen-lockfile
fi

if [ "$run_checks" = true ]; then
  pnpm typecheck
  pnpm lint
  pnpm test
  cargo test --manifest-path src-tauri/Cargo.toml --locked
  cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
  cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
fi

pnpm tauri build --bundles app

if [ ! -x "$binary" ]; then
  echo "Build finished without the expected executable: $binary" >&2
  exit 1
fi

if [ "$run_checks" = true ]; then
  python3 scripts/release-smoke.py "$binary"
fi

launch_path="$app"
if [ "$install_app" = true ]; then
  if pgrep -x git-view >/dev/null 2>&1; then
    echo 'Quit Git View before installing a replacement in /Applications.' >&2
    exit 1
  fi
  if [ ! -w /Applications ]; then
    echo 'Cannot write to /Applications. Copy the built app there with Finder.' >&2
    echo "Built app: $app" >&2
    exit 1
  fi
  install_stage="/Applications/.git-view-install-$$.app"
  install_backup="/Applications/.git-view-backup-$$.app"
  rm -rf -- "$install_stage" "$install_backup"
  if ! ditto "$app" "$install_stage"; then
    rm -rf -- "$install_stage"
    echo 'Could not prepare the application copy in /Applications.' >&2
    exit 1
  fi
  if [ -e "$install_path" ] && ! mv "$install_path" "$install_backup"; then
    rm -rf -- "$install_stage"
    echo 'Could not move the existing application aside.' >&2
    exit 1
  fi
  if ! mv "$install_stage" "$install_path"; then
    if [ -e "$install_backup" ]; then
      mv "$install_backup" "$install_path"
    fi
    rm -rf -- "$install_stage"
    echo 'Could not install the new application; the previous copy was restored.' >&2
    exit 1
  fi
  rm -rf -- "$install_backup"
  launch_path="$install_path"
  echo "Installed: $install_path"
fi

echo "Built: $app"

if [ "$open_app" = true ]; then
  open "$launch_path"
else
  echo "Open with: open '$launch_path'"
fi
