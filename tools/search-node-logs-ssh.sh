#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  search-node-logs-ssh.sh --since INTERVAL [--jobs N] <node-index> [<node-index> ...] -- "<grep command>"

Examples:
  search-node-logs-ssh.sh --since 10m {1..9} -- 'grep "ERROR"'
  search-node-logs-ssh.sh --since 2h --jobs 3 1 2 3 4 -- 'grep -E "timeout|panic"'

This script SSHes to each host:
  ubuntu@tpc-c-benchmark-spacetimedb-<node-index>

On each remote host it:
  1. Reads logs from the `spacetimedb` Docker container with `sudo docker logs`
  2. Uses `--since INTERVAL`
  3. Pipes the logs through the provided grep command
EOF
}

jobs=1
since=""

if [[ $# -eq 0 ]]; then
  usage
  exit 1
fi

while [[ $# -gt 0 ]]; do
  case "${1:-}" in
    --since)
      since="${2:-}"
      if [[ -z "$since" ]]; then
        echo "--since requires a value" >&2
        exit 1
      fi
      shift 2
      ;;
    --since=*)
      since="${1#*=}"
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

if [[ -z "$since" ]]; then
  echo "--since is required" >&2
  exit 1
fi

if ! [[ "$jobs" =~ ^[1-9][0-9]*$ ]]; then
  echo "--jobs must be a positive integer" >&2
  exit 1
fi

node_indices=()
while [[ $# -gt 0 ]]; do
  if [[ "$1" == "--" ]]; then
    shift
    break
  fi
  node_indices+=("$1")
  shift
done

if [[ ${#node_indices[@]} -eq 0 ]]; then
  echo "at least one node index is required" >&2
  usage
  exit 1
fi

if [[ $# -eq 0 ]]; then
  echo "a grep command is required after --" >&2
  usage
  exit 1
fi

grep_command="$*"

run_node() {
  local node_index="$1"
  local host="ubuntu@tpc-c-benchmark-spacetimedb-${node_index}"
  local remote_script

  remote_script=$(cat <<EOF
set -euo pipefail

since=$(printf '%q' "$since")
grep_command=$(printf '%q' "$grep_command")

container="spacetimedb"

if ! sudo docker ps --format '{{.Names}}' | grep -Fxq "\$container"; then
  echo "[$host] container \$container is not running"
  exit 0
fi

echo "[$host][\$container] searching logs since \$since"
if sudo docker logs --since "\$since" "\$container" 2>&1 | bash -lc "\$grep_command"; then
  :
else
  status=\$?
  if [[ \$status -eq 141 ]]; then
    echo "[$host][\$container] grep stopped early after finding requested matches"
  elif [[ \$status -eq 1 ]]; then
    echo "[$host][\$container] no matches"
  else
    echo "[$host][\$container] grep command failed with exit code \$status" >&2
    exit \$status
  fi
fi
EOF
)

  echo "==> ${host}"
  ssh "$host" "bash -lc $(printf '%q' "$remote_script")"
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

for node_index in "${node_indices[@]}"; do
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
