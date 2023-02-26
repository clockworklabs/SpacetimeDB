#!/bin/bash
set -euo pipefail

if [ "$#" -lt "1" ] ; then
  echo "Usage: $0 <test-name>"
  exit 1
fi

cd "$(dirname "$0")"

# sqlite vs spacetime
cargo build --release
bench="../../target/release/bench"
cargo flamegraph --deterministic --notes "sqlite ${1}"     -o sqlite.svg    -- --db sqlite ${1}
cargo flamegraph --deterministic --notes "spacetime ${1}"  -o spacetime.svg -- --db spacetime ${1}
