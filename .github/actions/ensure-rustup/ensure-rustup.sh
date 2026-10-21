#!/usr/bin/env bash
set -euo pipefail

minimum_version=1.29.2

version_is_current() {
  local version="${1#rustup }"
  version="${version%% *}"
  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || return 1

  local current minimum index
  IFS=. read -r -a current <<< "$version"
  IFS=. read -r -a minimum <<< "$minimum_version"
  for index in 0 1 2; do
    if (( 10#${current[$index]} > 10#${minimum[$index]} )); then
      return 0
    elif (( 10#${current[$index]} < 10#${minimum[$index]} )); then
      return 1
    fi
  done
}

if command -v rustup >/dev/null 2>&1; then
  before="$(rustup --version)"
  echo "Rustup before: $before"
  if ! version_is_current "$before"; then
    echo "Updating rustup to meet minimum version $minimum_version"
    rustup self update
  fi
else
  echo "Rustup before: not installed"
  cargo_home="${CARGO_HOME:-${USERPROFILE:-$HOME}/.cargo}"
  if [[ "${RUNNER_OS:-}" == Windows ]]; then
    cargo_home="$(cygpath -u "$cargo_home")"
    install_dir="$(mktemp -d)"
    trap 'rm -rf "$install_dir"' EXIT
    case "${RUNNER_ARCH:-X64}" in
      X64) host_arch=x86_64 ;;
      ARM64) host_arch=aarch64 ;;
      *) echo "Unsupported Windows architecture: $RUNNER_ARCH" >&2; exit 1 ;;
    esac
    curl --proto '=https' --tlsv1.2 --retry 3 -fsSL \
      "https://win.rustup.rs/$host_arch" -o "$install_dir/rustup-init.exe"
    "$install_dir/rustup-init.exe" -y --no-modify-path --default-toolchain none
  else
    curl --proto '=https' --tlsv1.2 --retry 3 -fsSL https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --default-toolchain none
  fi
  export PATH="$cargo_home/bin:$PATH"
  echo "$cargo_home/bin" >> "$GITHUB_PATH"
fi

after="$(rustup --version)"
echo "Rustup after: $after"
if ! version_is_current "$after"; then
  echo "::error::Rustup must be at least $minimum_version after installation or update"
  exit 1
fi
