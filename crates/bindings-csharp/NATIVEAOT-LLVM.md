# Using NativeAOT-LLVM with SpacetimeDB C# Modules

This guide provides instructions for enabling NativeAOT-LLVM compilation for C# SpacetimeDB modules, which can provide performance improvements.

## Overview

NativeAOT-LLVM compiles C# modules to native WebAssembly (WASM) instead of using the Mono runtime.

> [!WARNING]
> This is currently only supported for Windows server modules and is experimental.

## Prerequisites

- **.NET SDK 8.x** (same version used by SpacetimeDB)
- **Emscripten SDK (EMSDK)** installed (must contain `upstream/emscripten/emcc.bat`)
- **(Optional) Binaryen (wasm-opt)** installed and on `PATH` (recommended: `version_116`)
- **Windows** - NativeAOT-LLVM is currently only supported for Windows server modules

## Prerequisites Installation

### Install Emscripten SDK (EMSDK)

The Emscripten SDK is required for NativeAOT-LLVM compilation:

1. **Download and extract** the Emscripten SDK from `https://github.com/emscripten-core/emsdk`
   - Example path: `D:\Tools\emsdk`

2. **Set environment variable** (optional - the CLI will detect it automatically):
   ```
   $env:EMSDK="D:\Tools\emsdk"
   ```

### Install Binaryen (Optional)

Binaryen provides `wasm-opt` for WASM optimization (recommended for performance):

1. Download Binaryen https://github.com/WebAssembly/binaryen/releases/tag/version_116 for Windows
2. Extract to e.g. `D:\Tools\binaryen`
3. Add `D:\Tools\binaryen\bin` to `PATH`
   
   To temporarily add to your current PowerShell session:
   ```
   $env:PATH += ";D:\Tools\binaryen\bin"
   ```
4. Verify:
   ```
   wasm-opt --version
   ```

## Creating a New NativeAOT Project

When creating a new C# project, use the `--native-aot` flag:

```
spacetime init --lang csharp --native-aot my-native-aot-project
```

This automatically:
- Creates a C# project with the required package references
- Generates a `spacetime.json` with `"native-aot": true`
- Configures the project for NativeAOT-LLVM compilation

## Converting an Existing Project

1. **Update spacetime.json**
   Add `"native-aot": true` to your `spacetime.json`:
   ```json
   {
     "module": "your-module-name",
     "native-aot": true
   }
   ```
   
   **Note:** Once `spacetime.json` has `"native-aot": true`, you can simply run `spacetime publish` without the `--native-aot` flag. The CLI will automatically detect the configuration and use NativeAOT compilation.

2. **Ensure NuGet feed is configured**
   NativeAOT-LLVM packages come from **dotnet-experimental**. Add to `NuGet.Config`:
   ```xml
   <?xml version="1.0" encoding="utf-8"?>
   <configuration>
     <packageSources>
       <clear />
       <add key="dotnet-experimental" value="https://pkgs.dev.azure.com/dnceng/public/_packaging/dotnet-experimental/nuget/v3/index.json" />
       <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
     </packageSources>
   </configuration>
   ```

3. **Add NativeAOT package references**
   Add this `ItemGroup` to your `.csproj`:
   ```xml
   <ItemGroup Condition="'$(EXPERIMENTAL_WASM_AOT)' == '1'">
     <PackageReference Include="Microsoft.NET.ILLink.Tasks" Version="8.0.0-*" Condition="'$(ILLinkTargetsPath)' == ''" />
     <PackageReference Include="Microsoft.DotNet.ILCompiler.LLVM" Version="8.0.0-*" />
     <PackageReference Include="runtime.$(NETCoreSdkPortableRuntimeIdentifier).Microsoft.DotNet.ILCompiler.LLVM" Version="8.0.0-*" />
   </ItemGroup>
   ```

   Your complete `.csproj` should look like:
   ```xml
   <Project Sdk="Microsoft.NET.Sdk">
     <PropertyGroup>
       <TargetFramework>net8.0</TargetFramework>
       <RuntimeIdentifier>wasi-wasm</RuntimeIdentifier>
       <ImplicitUsings>enable</ImplicitUsings>
       <Nullable>enable</Nullable>
     </PropertyGroup>
     <ItemGroup>
       <PackageReference Include="SpacetimeDB.Runtime" Version="2.0.*" />
     </ItemGroup>
     <ItemGroup Condition="'$(EXPERIMENTAL_WASM_AOT)' == '1'">
       <PackageReference Include="Microsoft.NET.ILLink.Tasks" Version="8.0.0-*" Condition="'$(ILLinkTargetsPath)' == ''" />
       <PackageReference Include="Microsoft.DotNet.ILCompiler.LLVM" Version="8.0.0-*" />
       <PackageReference Include="runtime.$(NETCoreSdkPortableRuntimeIdentifier).Microsoft.DotNet.ILCompiler.LLVM" Version="8.0.0-*" />
     </ItemGroup>
   </Project>
   ```

## Publishing Your NativeAOT Module

After completing either the **Creating a New NativeAOT Project** or **Converting an Existing Project** steps above, you can publish your module normally:

```
# From your project directory
spacetime publish your-database-name
```

If you have `"native-aot": true` in your `spacetime.json`, the CLI will automatically detect this and use NativeAOT compilation. Alternatively, you can use:

```
spacetime publish --native-aot your-database-name
```

The CLI will display "Using NativeAOT-LLVM compilation (experimental)" when NativeAOT is enabled.

## Troubleshooting

### Package source mapping enabled
If you have **package source mapping** enabled in `NuGet.Config`, add mappings for the LLVM packages:

```xml
<packageSourceMapping>
    <packageSource key="bsatn-runtime">
        <package pattern="SpacetimeDB.BSATN.Runtime" />
    </packageSource>
    <packageSource key="SpacetimeDB.Runtime">
        <package pattern="SpacetimeDB.Runtime" />
    </packageSource>
    <packageSource key="dotnet-experimental">
        <package pattern="Microsoft.DotNet.ILCompiler.LLVM" />
        <package pattern="runtime.*" />
    </packageSource>
    <packageSource key="nuget.org">
        <package pattern="*" />
    </packageSource>
</packageSourceMapping>
```

### wasi-experimental workload install fails
If the CLI cannot install the `wasi-experimental` workload automatically, install it manually:

```
dotnet workload install wasi-experimental
```

### Duplicate PackageReference warning
You may see a `NU1504` warning about duplicate `PackageReference` items. This is expected and non-blocking.

### Code generation failed
If you see errors like "Code generation failed for method", ensure:
1. You're using `SpacetimeDB.Runtime` version 2.0.4 or newer
2. All required package references are in your `.csproj`
3. The `dotnet-experimental` feed is configured in `NuGet.Config`

