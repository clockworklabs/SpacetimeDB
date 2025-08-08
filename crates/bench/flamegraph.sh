#!/bin/bash
set -euo pipefail

if [ "$#" -lt "2" ] ; then
  echo "Usage: $0 <benchmark-exe> <benchmark-filter> <svg-name>"
  echo "E.g.: $0 generic stdb_raw/mem/insert_bulk/location/multi_index/load=0/count=100 result.svg"
  exit 1
fi

cd "$(dirname "$0")"

echo
echo "Warning(jgilles): this script has not been tested since its last modification, sorry if it is broken."
echo
echo cargo bench --no-run
echo

cargo bench --no-run

echo
echo cargo flamegraph --bench "${1}" --deterministic --notes "${1};${2}" -o "${3}" -- "${2}" --profile-time 10
echo

cargo flamegraph --bench "${1}" --deterministic --notes "${1};${2}" -o "${3}" -- "${2}" --profile-time 10
