#!/bin/bash
set -euo pipefail

if [ "$#" -lt "1" ] ; then
  echo "Usage: $0 <engine>"
  exit 1
fi

cd "$(dirname "$0")"

#Run in sequence
cargo run --color=always './test/**/*.slt' --engine $1