#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_DIR="$ROOT_DIR/sdks/swift/"

usage() {
  cat <<'EOF'
Usage:
  tools/swift-package-mirror.sh sync --mirror <path> [--allow-dirty] [--dry-run]
  tools/swift-package-mirror.sh release --mirror <path> --version <semver> [--allow-dirty] [--push]

Commands:
  sync      Mirror sdks/swift into a standalone package-root repository.
  release   Sync + commit + tag (optionally push) in the mirror repository.

Examples:
  tools/swift-package-mirror.sh sync --mirror ../spacetimedb-swift
  tools/swift-package-mirror.sh release --mirror ../spacetimedb-swift --version 0.1.0 --push
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

ensure_clean_repo() {
  local repo="$1"
  local allow_dirty="$2"
  if [[ "$allow_dirty" == "1" ]]; then
    return 0
  fi
  if [[ -n "$(git -C "$repo" status --porcelain)" ]]; then
    die "mirror repo has local changes; commit/stash first or use --allow-dirty"
  fi
}

sync_repo() {
  local mirror_repo="$1"
  local dry_run="$2"

  [[ -d "$mirror_repo/.git" ]] || die "mirror path is not a git repository: $mirror_repo"

  local rsync_flags=(-a --delete)
  if [[ "$dry_run" == "1" ]]; then
    rsync_flags+=(-n -v)
  fi

  rsync \
    "${rsync_flags[@]}" \
    --exclude '.git' \
    --exclude '.build' \
    --exclude '.swiftpm' \
    --exclude '.DS_Store' \
    "$SOURCE_DIR" \
    "$mirror_repo/"
}

create_release() {
  local mirror_repo="$1"
  local version="$2"
  local push="$3"

  local tag="v$version"
  if git -C "$mirror_repo" rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    die "tag already exists in mirror repo: $tag"
  fi

  if [[ -n "$(git -C "$mirror_repo" status --porcelain)" ]]; then
    git -C "$mirror_repo" add -A
    git -C "$mirror_repo" commit -m "release: SpacetimeDB Swift SDK $tag"
  else
    echo "No synced file changes detected; tagging current HEAD."
  fi

  git -C "$mirror_repo" tag -a "$tag" -m "SpacetimeDB Swift SDK $tag"

  if [[ "$push" == "1" ]]; then
    git -C "$mirror_repo" push origin HEAD
    git -C "$mirror_repo" push origin "$tag"
  fi
}

main() {
  require_cmd git
  require_cmd rsync

  if [[ $# -lt 1 ]]; then
    usage
    exit 64
  fi

  if [[ "$1" == "-h" || "$1" == "--help" || "$1" == "help" ]]; then
    usage
    exit 0
  fi

  local cmd="$1"
  shift

  local mirror_repo=""
  local version=""
  local allow_dirty="0"
  local dry_run="0"
  local push="0"

  while [[ $# -gt 0 ]]; do
    case "$1" in
    --mirror)
      [[ $# -ge 2 ]] || die "--mirror requires a value"
      mirror_repo="$2"
      shift 2
      ;;
    --version)
      [[ $# -ge 2 ]] || die "--version requires a value"
      version="$2"
      shift 2
      ;;
    --allow-dirty)
      allow_dirty="1"
      shift
      ;;
    --dry-run)
      dry_run="1"
      shift
      ;;
    --push)
      push="1"
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
    esac
  done

  [[ -n "$mirror_repo" ]] || die "--mirror is required"

  mirror_repo="$(cd "$mirror_repo" && pwd)"

  case "$cmd" in
  sync)
    ensure_clean_repo "$mirror_repo" "$allow_dirty"
    sync_repo "$mirror_repo" "$dry_run"
    echo "Mirror sync complete: $mirror_repo"
    ;;
  release)
    [[ -n "$version" ]] || die "--version is required for release"
    [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.-]+)?$ ]] || die "--version must look like semver (example: 1.2.3)"
    ensure_clean_repo "$mirror_repo" "$allow_dirty"
    sync_repo "$mirror_repo" "0"
    create_release "$mirror_repo" "$version" "$push"
    echo "Release prepared in mirror repo: $mirror_repo"
    echo "Version: v$version"
    ;;
  *)
    die "unknown command: $cmd"
    ;;
  esac
}

main "$@"
