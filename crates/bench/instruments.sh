#!/bin/bash
set -euo pipefail

if [ "$#" -lt "3" ] ; then
  echo "Usage: $0 <template> <bench_executable> <bench_filter>"
  echo "E.g.: $0 time generic stdb_raw/mem/insert_bulk/location/multi_index/load=0/count=100
"
  exit 1
fi

echo
echo cargo instruments -t "${1}" --bench "${2}" -- "${3}" --profile-time 10
echo

# Only OSX: Run the benchmark in instruments.app
cargo instruments -t "${1}" --bench "${2}" -- "${3}" --profile-time 10