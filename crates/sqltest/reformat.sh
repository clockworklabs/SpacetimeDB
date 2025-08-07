#!/bin/bash
set -euo pipefail

if [ "$#" -lt "1" ] ; then
  echo "Usage: $0 <glob>"
  exit 1
fi

cd "$(dirname "$0")"

cargo run --color=always $1 --format