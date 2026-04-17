#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  cleanup-orphan-replicas-ssh.sh [--dry-run] [--jobs N] <node-index> [<node-index> ...]

Examples:
  cleanup-orphan-replicas-ssh.sh 1 2 3
  cleanup-orphan-replicas-ssh.sh --dry-run 4 5 6
  cleanup-orphan-replicas-ssh.sh --jobs 3 {1..9}

This script SSHes to each host:
  ubuntu@tpc-c-benchmark-spacetimedb-<node-index>

On each remote host it:
  1. Queries live replica IDs from spacetime-control
  2. Compares them with directories under /stdb/replicas
  3. Writes ~/live-replicas and ~/replicas-to-delete
  4. Deletes orphaned replica directories unless --dry-run is set
EOF
}

dry_run=0
jobs=1

if [[ $# -eq 0 ]]; then
  usage
  exit 1
fi

while [[ $# -gt 0 ]]; do
  case "${1:-}" in
    --dry-run)
      dry_run=1
      shift
      ;;
    --jobs)
      jobs="${2:-}"
      if [[ -z "$jobs" ]]; then
        echo "--jobs requires a value" >&2
        exit 1
      fi
      shift 2
      ;;
    --jobs=*)
      jobs="${1#*=}"
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    -*)
      echo "unknown option: $1" >&2
      usage
      exit 1
      ;;
    *)
      break
      ;;
  esac
done

if ! [[ "$jobs" =~ ^[1-9][0-9]*$ ]]; then
  echo "--jobs must be a positive integer" >&2
  exit 1
fi

if [[ $# -eq 0 ]]; then
  usage
  exit 1
fi

remote_script='
set -euo pipefail

total_local_replicas="$(
  find /stdb/replicas -mindepth 1 -maxdepth 1 -type d | wc -l | awk "{print \$1}"
)"

spacetime sql -s local spacetime-control "SELECT id FROM replica" \
  | tail -n +3 \
  | awk '\''{$1=$1;print}'\'' \
  | sort -u \
  > "$HOME/live-replicas"

find /stdb/replicas -mindepth 1 -maxdepth 1 -type d -exec basename {} \; \
  | sort \
  | comm -23 - "$HOME/live-replicas" \
  > "$HOME/replicas-to-delete"

replicas_to_delete_count="$(wc -l < "$HOME/replicas-to-delete" | awk "{print \$1}")"
live_replicas_count="$(wc -l < "$HOME/live-replicas" | awk "{print \$1}")"

echo "total local replicas: $total_local_replicas"
echo "live replicas in control db: $live_replicas_count"
echo "replicas that would be deleted locally: $replicas_to_delete_count"

if [[ '"$dry_run"' -eq 1 ]]; then
  echo "dry run; would delete $replicas_to_delete_count replica directories:"
  cat "$HOME/replicas-to-delete"
else
  if [[ -s "$HOME/replicas-to-delete" ]]; then
    while IFS= read -r replica_id; do
      [[ -z "$replica_id" ]] && continue
      replica_path="/stdb/replicas/$replica_id"
      echo "deleting $replica_path"
      if [[ -d "$replica_path" ]]; then
        rm -rf -- "$replica_path"
        echo "deleted $replica_path"
      else
        echo "skipping missing $replica_path"
      fi
    done < "$HOME/replicas-to-delete"
    echo "deleted $replicas_to_delete_count replica directories listed in $HOME/replicas-to-delete"
  else
    echo "nothing to delete"
  fi
fi
'

run_node() {
  local node_index="$1"
  local host="ubuntu@tpc-c-benchmark-spacetimedb-${node_index}"

  echo "==> ${host}"
  ssh -oStrictHostKeyChecking=accept-new "$host" "bash -lc $(printf '%q' "$remote_script")"
}

running_pids=()

reap_finished_jobs() {
  local remaining=()
  local pid

  for pid in "${running_pids[@]}"; do
    if kill -0 "$pid" 2>/dev/null; then
      remaining+=("$pid")
    else
      wait "$pid"
    fi
  done

  if (( ${#remaining[@]} > 0 )); then
    running_pids=("${remaining[@]}")
  else
    running_pids=()
  fi
}

for node_index in "$@"; do
  run_node "$node_index" &
  running_pids+=("$!")

  while (( ${#running_pids[@]} >= jobs )); do
    sleep 0.1
    reap_finished_jobs
  done
done

if (( ${#running_pids[@]} > 0 )); then
  for pid in "${running_pids[@]}"; do
    wait "$pid"
  done
fi
