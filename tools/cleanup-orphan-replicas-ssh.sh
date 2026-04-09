#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  cleanup-orphan-replicas-ssh.sh [--dry-run] <node-index> [<node-index> ...]

Examples:
  cleanup-orphan-replicas-ssh.sh 1 2 3
  cleanup-orphan-replicas-ssh.sh --dry-run 4 5 6

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

if [[ $# -eq 0 ]]; then
  usage
  exit 1
fi

if [[ "${1:-}" == "--dry-run" ]]; then
  dry_run=1
  shift
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

for node_index in "$@"; do
  host="ubuntu@tpc-c-benchmark-spacetimedb-${node_index}"
  echo "==> ${host}"
  ssh "$host" "bash -lc $(printf '%q' "$remote_script")"
done
