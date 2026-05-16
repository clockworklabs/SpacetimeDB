#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../../.." && pwd)"
BUILD_DIR="${REPO_ROOT}/crates/bindings-cpp/build"
CMAKE_EXE="${CMAKE_EXE:-}"

if [[ -z "${CMAKE_EXE}" ]]; then
  if command -v cmake >/dev/null 2>&1; then
    CMAKE_EXE="cmake"
  else
    for candidate in \
      "/c/Program Files/CMake/bin/cmake.exe" \
      "/c/Strawberry/c/bin/cmake.exe"
    do
      if [[ -x "${candidate}" ]]; then
        CMAKE_EXE="${candidate}"
        break
      fi
    done
  fi
fi

if [[ -z "${CMAKE_EXE}" ]]; then
  echo "Could not find cmake. Set CMAKE_EXE to its full path." >&2
  exit 1
fi

"${CMAKE_EXE}" -S "${REPO_ROOT}/crates/bindings-cpp" -B "${BUILD_DIR}" -DBUILD_TESTS=ON
"${CMAKE_EXE}" --build "${BUILD_DIR}" --target query_builder_sql_tests

if [[ -x "${BUILD_DIR}/Debug/query_builder_sql_tests.exe" ]]; then
  "${BUILD_DIR}/Debug/query_builder_sql_tests.exe"
elif [[ -x "${BUILD_DIR}/Release/query_builder_sql_tests.exe" ]]; then
  "${BUILD_DIR}/Release/query_builder_sql_tests.exe"
elif [[ -x "${BUILD_DIR}/query_builder_sql_tests" ]]; then
  "${BUILD_DIR}/query_builder_sql_tests"
else
  echo "Could not find query_builder_sql_tests binary in ${BUILD_DIR}" >&2
  exit 1
fi
