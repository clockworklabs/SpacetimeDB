#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  run-on-nodes.sh [--jobs N] [--exit-on-error] <node-index> [<node-index> ...] -- "<command>"

Examples:
  run-on-nodes.sh 1 2 3 -- 'hostname'
  run-on-nodes.sh --jobs 3 {1..9} -- 'sudo docker ps'
  run-on-nodes.sh --jobs=4 2 4 6 -- 'cd /var/log && ls -lah'
  run-on-nodes.sh --jobs 3 --exit-on-error {1..9} -- 'systemctl is-active spacetimedb'

This script SSHes to each host:
  ubuntu@tpc-c-benchmark-spacetimedb-<node-index>

It runs the provided command on each node with configurable parallelism and
prints the output grouped by node.
EOF
}

jobs=1
exit_on_error=0

if [[ $# -eq 0 ]]; then
  usage
  exit 1
fi

while [[ $# -gt 0 ]]; do
  case "${1:-}" in
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
    --exit-on-error)
      exit_on_error=1
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
  echo "a command is required after --" >&2
  usage
  exit 1
fi

command="$*"
tmpdir="$(mktemp -d)"
overall_status=0

cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

run_node() {
  local node_index="$1"
  local host="ubuntu@tpc-c-benchmark-spacetimedb-${node_index}"
  local output_file="$tmpdir/${node_index}.out"
  local status_file="$tmpdir/${node_index}.status"
  local status

  set +e
  {
    echo "==> ${host}"
    ssh -oStrictHostKeyChecking=accept-new "$host" "bash -lc $(printf '%q' "$command")"
  } >"$output_file" 2>&1
  status="$?"
  set -e

  printf '%s\n' "$status" >"$status_file"
}

print_node_output() {
  local node_index="$1"
  local host="ubuntu@tpc-c-benchmark-spacetimedb-${node_index}"
  local output_file="$tmpdir/${node_index}.out"
  local status_file="$tmpdir/${node_index}.status"
  local status

  status="$(<"$status_file")"
  printf '===== %s (exit %s) =====\n' "$host" "$status"
  cat "$output_file"
  printf '\n'
  printf '===== end %s =====\n' "$host"

  if [[ "$status" -ne 0 ]]; then
    overall_status=1
  fi
}

running_pids=()
running_nodes=()
stop_scheduling=0

reap_finished_jobs() {
  local remaining_pids=()
  local remaining_nodes=()
  local i
  local pid
  local node_index

  for i in "${!running_pids[@]}"; do
    pid="${running_pids[$i]}"
    node_index="${running_nodes[$i]}"

    if kill -0 "$pid" 2>/dev/null; then
      remaining_pids+=("$pid")
      remaining_nodes+=("$node_index")
      continue
    fi

    wait "$pid" || true
    print_node_output "$node_index"

    if [[ "$exit_on_error" -eq 1 ]]; then
      if [[ "$(<"$tmpdir/${node_index}.status")" -ne 0 ]]; then
        stop_scheduling=1
      fi
    fi
  done

  if (( ${#remaining_pids[@]} > 0 )); then
    running_pids=("${remaining_pids[@]}")
    running_nodes=("${remaining_nodes[@]}")
  else
    running_pids=()
    running_nodes=()
  fi
}

for node_index in "${node_indices[@]}"; do
  if [[ "$stop_scheduling" -eq 1 ]]; then
    break
  fi

  run_node "$node_index" &
  running_pids+=("$!")
  running_nodes+=("$node_index")

  while (( ${#running_pids[@]} >= jobs )); do
    sleep 0.1
    reap_finished_jobs
    if [[ "$stop_scheduling" -eq 1 ]]; then
      break
    fi
  done
done

while (( ${#running_pids[@]} > 0 )); do
  sleep 0.1
  reap_finished_jobs
done

exit "$overall_status"
