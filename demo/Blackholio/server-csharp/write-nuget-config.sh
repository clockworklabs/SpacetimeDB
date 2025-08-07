#!/bin/bash

# Helper script to override dependencies to use a local clone of the SpacetimeDB repo.
# e.g. run:
# dotnet nuget locals all -c && dotnet pack ../../../SpacetimeDB/crates/bindings-csharp/BSATN.Runtime && dotnet pack ../../../SpacetimeDB/crates/bindings-csharp/Runtime && ./write-nuget-config.sh ../../../SpacetimeDB

set -ueo pipefail

SPACETIMEDB_REPO_PATH="$1"

cd "$(dirname "$(readlink -f "$0")")"

# Write out the nuget config file to `nuget.config`. This causes the spacetimedb-csharp-sdk repository
# to be aware of the local versions of the `bindings-csharp` packages in SpacetimeDB, and use them if
# available.
# See https://learn.microsoft.com/en-us/nuget/reference/nuget-config-file for more info on the config file,
# and https://tldp.org/LDP/abs/html/here-docs.html for more info on this bash feature.
cat >nuget.config <<EOF
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <!-- Local NuGet repositories -->
    <add key="Local SpacetimeDB.Runtime" value="${SPACETIMEDB_REPO_PATH}/crates/bindings-csharp/Runtime/bin/Release" />
    <add key="Local SpacetimeDB.BSATN.Runtime" value="${SPACETIMEDB_REPO_PATH}/crates/bindings-csharp/BSATN.Runtime/bin/Release" />
  </packageSources>
  <packageSourceMapping>
    <!-- Ensure that SpacetimeDB.BSATN.Runtime is used from the local folder. -->
    <!-- Otherwise we risk an outdated version being quietly pulled from NuGet for testing. -->
    <packageSource key="Local SpacetimeDB.Runtime">
      <package pattern="SpacetimeDB.Runtime" />
    </packageSource>
    <packageSource key="Local SpacetimeDB.BSATN.Runtime">
      <package pattern="SpacetimeDB.BSATN.Runtime" />
    </packageSource>
    <!-- Fallback for other packages (e.g. test deps). -->
    <packageSource key="nuget.org">
      <package pattern="*" />
    </packageSource>
  </packageSourceMapping>
</configuration>
EOF

echo "Wrote nuget.config contents:"
cat nuget.config
