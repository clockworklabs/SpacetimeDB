#!/bin/bash
set -euo pipefail

# sqlite vs spacetime
./hyperfine.sh insert
./hyperfine.sh insert-bulk
./hyperfine.sh select-no-index
