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
total_dbs=10

$bench --db spacetime create-db $total_dbs
$bench --db sqlite create-db $total_dbs

# Add --show-output to see errors...
hyperfine --show-output  --parameter-scan db 0 $total_dbs --shell=none --export-json out.json --warmup 5 --runs $total_dbs "${bench} --db spacetime ${1} {db}" "${bench} --db sqlite ${1} {db}"