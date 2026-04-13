set -ueo pipefail

SPACETIMEDB_REPO_PATH="$(readlink -f "$1")"

cd "$(dirname "$(readlink -f "$0")")"
cd ..

# Write out the nuget config file to `nuget.config`. This causes the spacetimedb-csharp-sdk repository
# to be aware of the local versions of the `bindings-csharp` packages in SpacetimeDB, and use them if
# available.
# See https://learn.microsoft.com/en-us/nuget/reference/nuget-config-file for more info on the config file,
# and https://tldp.org/LDP/abs/html/here-docs.html for more info on this bash feature.
cat >NuGet.Config <<EOF
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <clear />
    <!-- Local NuGet repositories -->
    <add key="Local SpacetimeDB.BSATN.Runtime" value="${SPACETIMEDB_REPO_PATH}/crates/bindings-csharp/BSATN.Runtime/bin/Release" />
    <!-- We need to override the module runtime as well because the examples use it -->
    <add key="Local SpacetimeDB.Runtime" value="${SPACETIMEDB_REPO_PATH}/crates/bindings-csharp/Runtime/bin/Release" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
  <packageSourceMapping>
    <!-- Ensure that SpacetimeDB.BSATN.Runtime is used from the local folder. -->
    <!-- Otherwise we risk an outdated version being quietly pulled from NuGet for testing. -->
    <packageSource key="Local SpacetimeDB.BSATN.Runtime">
      <package pattern="SpacetimeDB.BSATN.Runtime" />
    </packageSource>
    <packageSource key="Local SpacetimeDB.Runtime">
      <package pattern="SpacetimeDB.Runtime" />
    </packageSource>
    <!-- Fallback for other packages (e.g. test deps). -->
    <packageSource key="nuget.org">
      <package pattern="*" />
    </packageSource>
  </packageSourceMapping>
</configuration>
EOF

cat >"${SPACETIMEDB_REPO_PATH}/NuGet.Config" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <clear />
    <!-- Local NuGet repositories -->
    <add key="Local SpacetimeDB.BSATN.Runtime" value="crates/bindings-csharp/BSATN.Runtime/bin/Release" />
    <!-- We need to override the module runtime as well because the examples use it -->
    <add key="Local SpacetimeDB.Runtime" value="crates/bindings-csharp/Runtime/bin/Release" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
  <packageSourceMapping>
    <!-- Ensure that SpacetimeDB.BSATN.Runtime is used from the local folder. -->
    <!-- Otherwise we risk an outdated version being quietly pulled from NuGet for testing. -->
    <packageSource key="Local SpacetimeDB.BSATN.Runtime">
      <package pattern="SpacetimeDB.BSATN.Runtime" />
    </packageSource>
    <packageSource key="Local SpacetimeDB.Runtime">
      <package pattern="SpacetimeDB.Runtime" />
    </packageSource>
    <!-- Fallback for other packages (e.g. test deps). -->
    <packageSource key="nuget.org">
      <package pattern="*" />
    </packageSource>
  </packageSourceMapping>
</configuration>
EOF

echo "Wrote sdks/csharp/NuGet.Config contents:"
cat NuGet.Config
