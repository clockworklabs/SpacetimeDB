#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
INCLUDE_DIR="$REPO_ROOT/crates/bindings-cpp/include"

detect_cxx() {
    if [ -n "${CXX:-}" ]; then
        echo "$CXX"
        return
    fi
    if command -v where.exe >/dev/null 2>&1; then
        local win_path
        win_path="$(where.exe clang++ 2>/dev/null | head -n 1 | tr -d '\r')"
        if [ -n "$win_path" ]; then
            if command -v cygpath >/dev/null 2>&1; then
                cygpath -u "$win_path"
                return
            fi
            if command -v wslpath >/dev/null 2>&1; then
                wslpath -u "$win_path"
                return
            fi
            echo "$win_path"
            return
        fi
    fi
    if command -v clang++ >/dev/null 2>&1; then
        echo "clang++"
        return
    fi
    if command -v clang++.exe >/dev/null 2>&1; then
        echo "clang++.exe"
        return
    fi
    if [ -x "/e/Program Files/LLVM/bin/clang++.exe" ]; then
        echo "/e/Program Files/LLVM/bin/clang++.exe"
        return
    fi
    if [ -x "/c/Program Files/LLVM/bin/clang++.exe" ]; then
        echo "/c/Program Files/LLVM/bin/clang++.exe"
        return
    fi
    echo "clang++"
}

CXX_BIN="$(detect_cxx)"

normalize_arg_path() {
    local path="$1"
    if [[ "$CXX_BIN" == *.exe ]]; then
        if command -v cygpath >/dev/null 2>&1; then
            cygpath -m "$path"
            return
        fi
        if command -v wslpath >/dev/null 2>&1; then
            wslpath -m "$path"
            return
        fi
    fi
    echo "$path"
}

compile_should_pass() {
    local file="$1"
    local include_dir
    include_dir="$(normalize_arg_path "$INCLUDE_DIR")"
    local input_file
    input_file="$(normalize_arg_path "$file")"
    echo "PASS  $(basename "$file")"
    "$CXX_BIN" -std=c++20 -fsyntax-only -I "$include_dir" "$input_file"
}

compile_should_fail() {
    local file="$1"
    local pattern="$2"
    local output
    local include_dir
    include_dir="$(normalize_arg_path "$INCLUDE_DIR")"
    local input_file
    input_file="$(normalize_arg_path "$file")"
    echo "FAIL  $(basename "$file")"
    if output="$("$CXX_BIN" -std=c++20 -fsyntax-only -I "$include_dir" "$input_file" 2>&1)"; then
        echo "Expected compile failure for $file"
        exit 1
    fi
    if ! grep -Fq "$pattern" <<<"$output"; then
        echo "Missing expected diagnostic for $file"
        echo "Expected pattern: $pattern"
        echo "$output"
        exit 1
    fi
}

compile_should_pass "$SCRIPT_DIR/pass_query_integration.cpp"
compile_should_fail "$SCRIPT_DIR/fail_non_index_join.cpp" "no member named 'tenant_id'"
compile_should_fail "$SCRIPT_DIR/fail_event_lookup.cpp" "Lookup side of a semijoin must opt in via CanBeLookupTable."

echo "All query-builder compile tests passed"
