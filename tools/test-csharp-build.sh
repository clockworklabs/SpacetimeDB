#!/usr/bin/env bash
set -euo pipefail

# Requires .NET 10, .NET 8 with wasi-experimental, jq, and a CLI built from this checkout.
repo="$(cd "$(dirname "$0")/.." && pwd)"
cli="${SPACETIME_BIN:?Set SPACETIME_BIN to the built spacetimedb-cli}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cd "$repo"

# A unique version prevents NuGet's cache from hiding changes to the runtime.
version="0.0.0-buildtest.$(date +%s).$$"
dotnet pack crates/bindings-csharp/BSATN.Runtime -c Release -p:Version="$version" -o "$work/packages"
dotnet pack crates/bindings-csharp/Runtime -c Release -p:Version="$version" -o "$work/packages"
cat > "$work/NuGet.Config" <<EOF
<configuration>
  <packageSources>
    <clear />
    <add key="local" value="$work/packages" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
    <add key="dotnet-experimental" value="https://pkgs.dev.azure.com/dnceng/public/_packaging/dotnet-experimental/nuget/v3/index.json" />
  </packageSources>
</configuration>
EOF

for template in basic-cs chat-console-cs; do
    echo "Checking $template managed/WASM builds"
    module="$work/$template/module"
    consumer="$work/$template/consumer"
    mkdir -p "$module" "$consumer"
    cp "templates/$template/spacetimedb/Lib.cs" "$module/"
    cp "templates/$template/spacetimedb/global.json" "$work/$template/"
    # Match the framework selection performed by spacetime init.
    sed 's/<TargetFrameworks>net8.0;net10.0<\/TargetFrameworks>/<TargetFramework>net10.0<\/TargetFramework>/' \
        "templates/$template/spacetimedb/StdbModule.csproj" > "$module/StdbModule.csproj"
    dotnet add "$module/StdbModule.csproj" package SpacetimeDB.Runtime --version "$version" --no-restore

    cat > "$consumer/Consumer.csproj" <<'EOF'
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net10.0</TargetFramework>
    <OutputType>Exe</OutputType>
    <ImplicitUsings>enable</ImplicitUsings>
  </PropertyGroup>
  <ItemGroup>
    <ProjectReference Include="../module/StdbModule.csproj" />
  </ItemGroup>
</Project>
EOF
    cat > "$consumer/Program.cs" <<'EOF'
using System.Reflection;
using System.Runtime.InteropServices;

var registration = typeof(Module).Assembly.GetType("ModuleRegistration", throwOnError: true)!;
if (registration.GetMethods().Any(method => method.IsDefined(typeof(UnmanagedCallersOnlyAttribute))))
    throw new Exception("Managed module contains WASM-only entry points");
Console.WriteLine("Managed module reference works");
EOF

    for phase in before-wasm after-wasm; do
        # A legacy/global AOT flag must not turn a .NET 10 managed build into WASM.
        SPACETIMEDB_WASM_AOT=1 dotnet run --project "$consumer/Consumer.csproj" -c Release
        jq -e '[.libraries | keys[] | select(contains("ILCompiler.LLVM"))] | length == 0' \
            "$module/obj/project.assets.json"
        if [ "$phase" = before-wasm ]; then
            "$cli" build --module-path "$module"
        fi
    done
done

# Ordinary test projects consume the module through either project or package references.
module="$work/chat-console-cs/module"
tests="$work/chat-console-cs/tests"
mkdir -p "$tests"
cat > "$tests/Module.Tests.csproj" <<'EOF'
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net10.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <IsTestProject>true</IsTestProject>
    <IsPackable>false</IsPackable>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Microsoft.NET.Test.Sdk" Version="17.6.0" />
    <PackageReference Include="xunit" Version="2.9.0" />
    <PackageReference Include="xunit.runner.visualstudio" Version="2.8.1" PrivateAssets="all" />
    <ProjectReference Include="../module/StdbModule.csproj" />
  </ItemGroup>
</Project>
EOF
cat > "$tests/ModuleReferenceTests.cs" <<'EOF'
using System.Reflection;
using SpacetimeDB;
using Xunit;

public class ModuleReferenceTests
{
    [Fact]
    public void ReferencesModuleWithoutBecomingAModule()
    {
        var testAssembly = typeof(ModuleReferenceTests).Assembly;
        Assert.Null(testAssembly.GetCustomAttribute<ModuleDescriptorAttribute>());
        Assert.Null(testAssembly.GetType("ModuleRegistration"));
    }

    // Compile-time checks only; accessing live data still requires the testing backend.
    public static ulong Count(ReducerContext ctx) => ctx.Db.Message.Count;
    public static ulong Count(ViewContext ctx) => ctx.Db.Message.Count;
    public static IQuery<global::Module.Message> Query(ViewContext ctx) => ctx.From.Message();
}
EOF
dotnet test "$tests/Module.Tests.csproj" -c Release
dotnet pack "$module/StdbModule.csproj" -c Release -p:PackageId=BuildTest.Module -p:Version="$version" -o "$work/packages"
dotnet remove "$tests/Module.Tests.csproj" reference "$module/StdbModule.csproj"
dotnet add "$tests/Module.Tests.csproj" package BuildTest.Module --version "$version"
dotnet test "$tests/Module.Tests.csproj" -c Release

# Exercise the .NET 8 SDK's WASI import order, not just our final property values.
module="$work/net8/module"
mkdir -p "$module"
cp templates/chat-console-cs/spacetimedb/Lib.cs "$module/"
sed 's/<TargetFramework>net10.0<\/TargetFramework>/<TargetFrameworks>net8.0;net10.0<\/TargetFrameworks>/' \
    "$work/chat-console-cs/module/StdbModule.csproj" > "$module/StdbModule.csproj"
cat > "$work/net8/global.json" <<'EOF'
{"sdk":{"version":"8.0.100","rollForward":"latestFeature"}}
EOF
"$cli" build --module-path "$module" --dotnet-version 8
