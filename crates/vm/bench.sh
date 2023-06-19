#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

cargo build --release && hyperfine --warmup 10 --shell=none --runs 50 'python3 fib.py' '../../target/release/vm' '../../target/release/vm native'
