#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

dotnet build "$REPO_ROOT/crates/bindings-csharp/BSATN.Runtime/BSATN.Runtime.csproj" \
  -c Release \
  -p:TargetFramework=net8.0 \
  -p:NuGetAudit=false \
  -p:RestoreIgnoreFailedSources=true

dotnet build \
  -p:NuGetAudit=false \
  -p:RestoreIgnoreFailedSources=true
