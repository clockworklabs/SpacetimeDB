# Using NativeAOT-LLVM with SpacetimeDB C# Modules

This guide provides instructions for enabling NativeAOT-LLVM compilation for C# SpacetimeDB modules, which can provide performance improvements by compiling C# directly to native WebAssembly (WASM) using the .NET NativeAOT-LLVM toolchain.

> [!WARNING]
> NativeAOT-LLVM is experimental.

## Overview

SpacetimeDB supports three build targets for C# modules:

| Build Target | .NET Version | Platforms | Description |
|--------------|--------------|-----------|-------------|
| **JIT (Mono)** | .NET 8.0 | Windows, Linux, macOS | Uses the Mono runtime interpreter (default) |
| **NativeAOT-LLVM** | .NET 8.0 | **Windows only** | Compiles C# to native WASM |
| **NativeAOT-LLVM** | .NET 10.0+ | Windows, Linux | Compiles C# to native WASM |

> [!NOTE]
> .NET 8.0 NativeAOT-LLVM is Windows-only because `runtime.linux-x64.Microsoft.DotNet.ILCompiler.LLVM` was never published to the dotnet-experimental feed.

## Prerequisites

- **.NET SDK 8.0** or **.NET SDK 10.0**
- **WASI SDK** (automatically downloaded during first AOT build)
- **(Optional) Binaryen (wasm-opt)** for WASM optimization

### WASI SDK (Auto-Downloaded)

The WASI SDK is required for NativeAOT-LLVM compilation and is **automatically downloaded**:

| Platform | Download Location |
|----------|-------------------|
| Windows | `%USERPROFILE%\.wasi-sdk\wasi-sdk-29` |
| Linux/macOS | `~/.wasi-sdk/wasi-sdk-29` |

Override with the `WASI_SDK_PATH` environment variable:

```bash
# Windows
$env:WASI_SDK_PATH="C:\Tools\wasi-sdk"

# Linux/macOS
export WASI_SDK_PATH=/opt/wasi-sdk
```

---

## Build Target: .NET 8.0 NativeAOT-LLVM (Windows Only)

For Windows users who want NativeAOT-LLVM compilation using .NET 8.0 SDK.

### Requirements
- .NET SDK 8.0
- Windows operating system
- NuGet.Config with dotnet-experimental feed

### Project Configuration

Your `.csproj` must include the conditional LLVM package references:

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <RuntimeIdentifier>wasi-wasm</RuntimeIdentifier>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="SpacetimeDB.Runtime" Version="2.2.*" />
  </ItemGroup>

  <!-- Required for .NET 8 AOT builds -->
  <ItemGroup Condition="'$(EXPERIMENTAL_WASM_AOT)' == '1'">
    <PackageReference Include="Microsoft.DotNet.ILCompiler.LLVM" Version="8.0.0-*" />
    <PackageReference Include="runtime.$(NETCoreSdkPortableRuntimeIdentifier).Microsoft.DotNet.ILCompiler.LLVM" Version="8.0.0-*" />
  </ItemGroup>
</Project>
```

Your `NuGet.Config` must include:

```xml
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <clear />
    <add key="dotnet-experimental" value="https://pkgs.dev.azure.com/dnceng/public/_packaging/dotnet-experimental/nuget/v3/index.json" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
  <packageSourceMapping>
    <packageSource key="dotnet-experimental">
      <package pattern="Microsoft.DotNet.ILCompiler.LLVM" />
      <package pattern="runtime.*" />
    </packageSource>
    <packageSource key="nuget.org">
      <package pattern="*" />
    </packageSource>
  </packageSourceMapping>
</configuration>
```

### Activating NativeAOT-LLVM (.NET 8)

There are three ways to enable NativeAOT-LLVM for .NET 8 builds.

**Option 1: `--native-aot` flag during init**
```
spacetime init --lang csharp --native-aot --dotnet-version 8 my-project
```

**Option 2: `--native-aot` flag during publish**
```
spacetime publish --native-aot my-database-name
```

**Option 3: `spacetime.json` configuration**
```json
{
  "module": "my-module",
  "native-aot": true
}
```

Technically all of these options just set the `EXPERIMENTAL_WASM_AOT` environment variable, but they provide different user experiences. Using `--native-aot` during `init` will create a project with a `spacetime.json` configured like Option 3 so the new project is consistently published with NativeAOT-LLVM.

---

## Build Target: .NET 10.0+ NativeAOT-LLVM (Windows & Linux)

For users who want NativeAOT-LLVM compilation on Windows **or** Linux.

### Requirements
- .NET SDK 10.0
- Windows or Linux operating system
- NuGet.Config with dotnet-experimental feed

### Project Configuration

For .NET 10, the project configuration is simpler - no conditional package references needed:

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net10.0</TargetFramework>
    <RuntimeIdentifier>wasi-wasm</RuntimeIdentifier>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="SpacetimeDB.Runtime" Version="2.2.*" />
  </ItemGroup>
</Project>
```

Your `NuGet.Config` must include:

```xml
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <clear />
    <add key="dotnet-experimental" value="https://pkgs.dev.azure.com/dnceng/public/_packaging/dotnet-experimental/nuget/v3/index.json" />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
  </packageSources>
  <packageSourceMapping>
    <packageSource key="dotnet-experimental">
      <package pattern="Microsoft.DotNet.ILCompiler.LLVM" />
      <package pattern="runtime.*" />
    </packageSource>
    <packageSource key="nuget.org">
      <package pattern="*" />
    </packageSource>
  </packageSourceMapping>
</configuration>
```

### global.json (if needed)

If .NET 10 is not your default SDK, create a `global.json`:

```json
{
  "sdk": {
    "version": "10.0.100",
    "rollForward": "latestMinor"
  }
}
```

This is automatically created by the CLI when using the `init` command with `--dotnet-version 10`.

### Activating NativeAOT-LLVM (.NET 10)

NativeAOT-LLVM is automatically used when targeting .NET 10. You can also explicitly enable it:

**Option 1: Target .NET 10 during init (recommended)**
```
spacetime init --lang csharp --dotnet-version 10 my-project
```

**Option 2: Use `--native-aot` flag**
```
spacetime init --lang csharp --native-aot my-project
```

**Option 3: `spacetime.json` configuration**
```json
{
  "module": "my-module",
  "native-aot": true
}
```

---

## Publishing Your Module

Once configured, publish normally:

```
spacetime publish my-database-name
```

The CLI will display which build path is being used:
- "Using NativeAOT-LLVM compilation (experimental)" for AOT builds
- Standard output for JIT builds

### Controlling the .NET Version During Publish

To explicitly publish with a specific .NET version:

```
# Force .NET 8 build (requires --native-aot for AOT)
spacetime publish --dotnet-version 8 --native-aot my-database-name

# Force .NET 10 build (automatically uses AOT)
spacetime publish --dotnet-version 10 my-database-name
```

---

## Troubleshooting

### WASI SDK not found

**Error**:
```
error : Could not find wasi-sdk. Either set $(WASI_SDK_PATH), or use workloads to get the sdk.
```

**Solution**: 
1. The WASI SDK should auto-download during first AOT build
2. If it fails, manually install from https://github.com/WebAssembly/wasi-sdk/releases
3. Set `WASI_SDK_PATH` environment variable
4. Restart your terminal/IDE

### .NET 8 AOT fails on Linux

**Error**: Missing `runtime.linux-x64.Microsoft.DotNet.ILCompiler.LLVM`

**Cause**: .NET 8 NativeAOT-LLVM packages were only published for Windows.

**Solution**: Use .NET 10 for Linux NativeAOT builds:
```
spacetime init --lang csharp --dotnet-version 10 my-project
```

### JIT builds fail: Missing wasi-experimental workload

For **JIT builds only** (not NativeAOT), you need the `wasi-experimental` workload:

```
dotnet workload install wasi-experimental
```

NativeAOT-LLVM builds do **not** use this workload; they use the WASI SDK instead.

### Code generation failed

If you see "Code generation failed for method" errors:
1. Ensure `NuGet.Config` includes the `dotnet-experimental` feed
2. For .NET 8: Verify the `EXPERIMENTAL_WASM_AOT` condition is in your `.csproj`
3. For .NET 10: Verify `TargetFramework` is `net10.0`
4. Check that `global.json` exists if .NET 10 is not your default SDK

### Duplicate PackageReference warning (NU1504)

This warning is expected for .NET 8 AOT builds and is non-blocking.

