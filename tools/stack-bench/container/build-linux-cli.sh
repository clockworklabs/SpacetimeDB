#!/usr/bin/env bash
# Export native binaries through the same Docker build used by the appliance.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
MSYS_NO_PATHCONV=1 docker build --platform linux/amd64 \
  --file "$REPO/tools/stack-bench/appliance/Controller.Dockerfile" \
  --target binary-export --output "type=local,dest=$HERE" "$REPO"
