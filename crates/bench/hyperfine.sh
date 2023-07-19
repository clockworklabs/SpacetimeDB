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
total_warmup=5
# How many Dbs to create, total_dbs + total_warmup
total_create=15

$bench --db spacetime create-db $total_create
$bench --db sqlite create-db $total_create

# Add --show-output to see errors...
hyperfine --shell=none --export-json out.json --warmup $total_warmup --runs $total_dbs "${bench} --db spacetime ${1}" "${bench} --db sqlite ${1}"