#!/usr/bin/env bash

set -euo pipefail

# Keep partition jobs independent of the large ci-sdk-tests helper binary so it
# does not have to be packaged, uploaded, and downloaded with every test build.
usage() {
  echo "Usage: $0 --archive-file PATH --module-dir PATH --client-dir PATH -- [nextest arguments]" >&2
  exit 2
}

archive_file=""
module_dir=""
client_dir=""

while (( $# > 0 )); do
  case "$1" in
    --archive-file)
      (( $# >= 2 )) || usage
      archive_file="$2"
      shift 2
      ;;
    --module-dir)
      (( $# >= 2 )) || usage
      module_dir="$2"
      shift 2
      ;;
    --client-dir)
      (( $# >= 2 )) || usage
      client_dir="$2"
      shift 2
      ;;
    --)
      shift
      break
      ;;
    *)
      usage
      ;;
  esac
done

[[ -n "${archive_file}" && -n "${module_dir}" && -n "${client_dir}" ]] || usage
[[ -f "${archive_file}" ]] || { echo "SDK test archive does not exist: ${archive_file}" >&2; exit 1; }
[[ -d "${module_dir}" ]] || { echo "SDK module directory does not exist: ${module_dir}" >&2; exit 1; }
[[ -d "${client_dir}" ]] || { echo "Prepared SDK client directory does not exist: ${client_dir}" >&2; exit 1; }
: "${SPACETIME_BIN:?SPACETIME_BIN must point to the spacetimedb CLI}"
[[ -f "${SPACETIME_BIN}" ]] || { echo "SpacetimeDB CLI does not exist: ${SPACETIME_BIN}" >&2; exit 1; }
[[ -f "$(dirname "${SPACETIME_BIN}")/spacetimedb-standalone" ]] || {
  echo "SpacetimeDB standalone runtime was not found alongside ${SPACETIME_BIN}" >&2
  exit 1
}

workspace_root="$(pwd -P)"
export SPACETIME_SDK_TEST_MODULE_DIR="$(realpath "${module_dir}")"
export SPACETIME_SDK_TEST_CLIENT_DIR="$(realpath "${client_dir}")"
export SPACETIME_SDK_TEST_WORKSPACE_ROOT="${workspace_root}"

cargo nextest run \
  --archive-file "$(realpath "${archive_file}")" \
  --workspace-remap "${workspace_root}" \
  --no-fail-fast \
  --no-tests pass \
  -j 1 \
  "$@"

bash tools/check-diff.sh
