# Converting a SpacetimeDB 2.0.x project to use NativeAOT-LLVM

This guide provides instructions on taking an existing C# module that targets the public-released SpacetimeDB CLI, and guides you through the necessary steps to enable `NativeAOT-LLVM` use.

## Overview
In order to use `NativeAOT-LLVM` on a C# module, we'll need to set the `EXPERIMENTAL_WASM_AOT` environment variable to `1` which SpacetimeDB will check during publishing of a module.
For the module to work, we'll also need the `NuGet.Config` and `.csproj` files with the required package sources and references.

### Prerequisites:
- **.NET SDK 8.x** (same version used by SpacetimeDB)
- **Emscripten SDK (EMSDK)** installed (must contain `upstream/emscripten/emcc.bat`)
- **(Optional) Binaryen (wasm-opt)** installed and on `PATH` (recommended: `version_116`)

## Steps

1. **Install EMSDK**
   - Download and extract the `https://github.com/emscripten-core/emsdk` release.
   - Example path: `D:\Tools\emsdk`

2. **Set environment variables**

   ```powershell
   $env:EXPERIMENTAL_WASM_AOT=1
   $env:EMSDK="D:\Tools\emsdk"
   ```

3. **Ensure NuGet feed is configured**
   NativeAOT-LLVM packages currently come from **dotnet-experimental**:
   - Add the `dotnet-experimental` feed to a project-local `NuGet.Config`
    ```xml
    <add key="dotnet-experimental" value="https://pkgs.dev.azure.com/dnceng/public/_packaging/dotnet-experimental/nuget/v3/index.json" />
    ```
    This should be a `NuGet.Config` placed in the root directory of your module folder (next to the `.csproj`). You can simply add the above line to the `packageSources` of your existing file, or if you need to create a minimal one, you can use:
    
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

4. **Ensure NativeAOT emits a `.wasm` output**
   - For LLVM AOT builds, the CLI currently accepts `dotnet.wasm` under `bin/Release/net8.0/wasi-wasm/publish/`.
   - In the module `.csproj`, ensure the AOT package references include:

    ```xml
    <ItemGroup Condition="'$(EXPERIMENTAL_WASM_AOT)' == '1'">
        <PackageReference Include="Microsoft.NET.ILLink.Tasks" Version="8.0.0-*" Condition="'$(ILLinkTargetsPath)' == ''" />
        <PackageReference Include="Microsoft.DotNet.ILCompiler.LLVM" Version="8.0.0-*" />
        <PackageReference Include="runtime.$(NETCoreSdkPortableRuntimeIdentifier).Microsoft.DotNet.ILCompiler.LLVM" Version="8.0.0-*" />
    </ItemGroup>
    ```
    
    The contents of your `.csproj` should look something like this:
    
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

   - **NU1504 warning**: Because the runtime targets also add these LLVM packages, you may see a duplicate PackageReference warning. It is non-blocking.

5. **(Optional) Install wasm-opt (Binaryen)**
   This step is optional, but provides performance improvements, and therefore is recommended.
   - Download Binaryen `https://github.com/WebAssembly/binaryen/releases/tag/version_116` for Windows and extract it, e.g. `D:\Tools\binaryen`.
   - Add `D:\Tools\binaryen\bin` to `PATH`.
   - Verify:

     ```powershell
     wasm-opt --version
     ```

6. **Publish module**
   - Use the SpacetimeDB CLI to publish from the module directory.
   - With `EXPERIMENTAL_WASM_AOT=1`, publish should attempt LLVM AOT.
   - Ensure the local server is running if publishing to `local`.

## Troubleshooting

### Package source mapping enabled
If you have **package source mapping** enabled in `NuGet.Config`, you must add mappings for the LLVM packages or restores will fail.
Place the following in `NuGet.Config` inside the `configuration` section:
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

```powershell
dotnet workload install wasi-experimental
```
