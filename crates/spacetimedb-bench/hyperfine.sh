#!/bin/bash
set -euo pipefail

if [ "$#" -lt "1" ] ; then
  echo "Usage: $0 <test-name>"
  exit 1
fi

cd "$(dirname "$0")"

# sqlite vs spacetime
cargo build --release
bench="../../target/release/spacetimedb-bench"
# Add --show-output to see errors...
hyperfine --shell=none --export-json out.json --warmup 1 --runs 1 "${bench} --db spacetime ${1}" "${bench} --db sqlite ${1}"