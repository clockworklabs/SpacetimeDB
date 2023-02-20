#!/bin/bash
set -euo pipefail

if [ "$#" -lt "3" ] ; then
  echo "Usage: $0 <engine> <template> <test-name>"
  exit 1
fi

# Only OSX: Run the benchmark in instruments.app
cargo instruments -t "${2}" -- --db "${1}" "${3}"