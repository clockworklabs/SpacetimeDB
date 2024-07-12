#!/bin/env bash

set -euo pipefail

script_dir="$(readlink -f "$(dirname "$0")")"
stdb_root="$(realpath "$script_dir/../")"

set -euox pipefail

cd "$stdb_root"

tools/clippy.sh

cargo test --all

if which python3 >/dev/null ; then
    python3 -m smoketests
elif which python >/dev/null ; then
    python -m smoketests
else
    echo "Can't find python, not running smoketests"
fi

if which dotnet >/dev/null ; then
    test crates/bindings-csharp
else
    echo "Can't find dotnet, not running smoketests"
fi

