# Developing SpacetimeDB.Testing

Contributor notes for the testing package. [README.md](README.md) is the NuGet
package readme and covers only the user-facing API.

Identity derivation uses the production Rust implementation through the native
test bridge, without adding a hashing dependency to Runtime. Rebuild the bridge
when changing that API.

## Packing

The package contains the managed testing assembly and staged native bridge binaries.
It depends on exactly the referenced Runtime project's package version, including
version overrides, and reuses Runtime's analyzer. Native loading and ABI validation
use the existing interop implementation. Platform builds and release distribution
are gated on the native CI matrix; local packing does not establish platform support.

Stage Release native artifacts in this layout:

```text
native/
  linux-x64/libspacetimedb_test_datastore_ffi.so
  linux-arm64/libspacetimedb_test_datastore_ffi.so
  win-x64/spacetimedb_test_datastore_ffi.dll
```

From the repository root:

```sh
dotnet pack crates/bindings-csharp/Testing -c Release \
  -p:TestingNativeArtifactsDir="$PWD/native"
```

All three assets are required by default and are packaged under
`runtimes/<rid>/native`. Packing does not compile Rust or inspect the binaries'
architecture; CI must build and execute them on each supported platform.

For a partial local package, explicitly opt in, select the RID, and use a
prerelease package version. Do not publish partial packages:

```sh
dotnet pack crates/bindings-csharp/Testing -c Release \
  -p:TestingNativeArtifactsDir="$PWD/native" \
  -p:TestingLocalPackage=true -p:TestingNativeRids=linux-x64 \
  -p:PackageVersion=2.13.0-local
```

Pack the matching Runtime and BSATN.Runtime packages with the same version
override when preparing a local feed. Native artifacts must come from the same
source revision as the managed package.

## Standalone Package Acceptance

Pull requests build, test and pack linux-x64 only, as a prerelease partial package.
Pushes to master, the merge queue and releases cover all three platforms.

The `csharp_testing_pack` GitHub Actions job assembles Testing with the tested
Release bridges, verifies their packaged bytes, and uploads a single
`spacetimedb-testing-packages` artifact with matching Runtime and BSATN.Runtime
packages. It also checks that Testing's nuspec pins the exact Runtime version.

The `csharp_testing_packages` matrix downloads that same package set on each
tested platform. It copies the chat template module and sibling test project
outside the checkout, restores with fresh NuGet caches,
and runs `dotnet test`. SpacetimeDB packages must come from the downloaded feed;
test-framework dependencies come from NuGet.org. Temporary files are removed
when the step finishes. The Test Suite aggregate requires the matrix to pass.

The chat tests cover fixture creation, caller identity and time, reducer commit
and rollback, indexed lookup, and disposal. They use the packaged analyzers and
native library without repository project references, manual native-library copying, or
`NativeTestLibrary`. The consumer workflow does not invoke Cargo or a WASI toolchain.
The `csharp_testing_native` matrix builds the Release bridge and runs the managed
integration suite natively on Linux x64, Linux ARM64, and Windows x64 before uploading
`spacetimedb-testing-native-<rid>` artifacts containing `<rid>/<native-library>`.
Linux builds use Ubuntu 22.04 (glibc 2.35); CI rejects newer glibc requirements.
Windows builds and tests use Windows Server 2022 with MSVC. These are the tested
OS baselines, not a claim of compatibility with older systems. macOS
is outside this matrix. CI logs include Linux native architecture and dependency reports.
The existing `csharp-testsuite` managed/WASM build check also consumes the same
package set. It checks WASM Runtime selection and rejects Testing dependencies
or native bridge files in module assets and build outputs. Platform support is
gated on successful execution of these jobs, not just package assembly.

## Release Integration

CI and releases share `.github/workflows/csharp-testing.yml`. It resolves the
source revision once, builds and validates all three native platforms, assembles
the complete package set, and runs package acceptance on each platform. The
release workflow cannot publish until the reusable workflow succeeds.

Release packages use the release tag without its leading `v`. Ordinary prerelease
suffixes are preserved; `-hotfix<N>` is stripped to retain the existing NuGet
hotfix convention. CI uses a unique `0.0.0-packagetest.*` version instead.

The release job downloads `spacetimedb-testing-packages` and supplies its directory
through `CSHARP_PACKAGE_FEED`. `cargo-release` requires all three module packages,
including Testing, and publishes those files unchanged. ClientSDK and Unity
preparation use the same feed instead of repacking Runtime and BSATN.Runtime;
their existing client build and publication flow otherwise remains unchanged.
Manual release commands also require this validated feed, with no build fallback.

Dry runs perform the same native and package validation and prepare ClientSDK
outputs, but do not push NuGet packages or commit/publish the Unity mirror.

## Repository Tests

From the repository root, build the native bridge and run the integration tests
(Linux example):

```sh
cargo build -p spacetimedb-test-datastore-ffi
dotnet test crates/bindings-csharp/Testing.Tests/Testing.Tests.csproj \
  -p:NativeTestLibrary="$PWD/target/debug/libspacetimedb_test_datastore_ffi.so"
```

`NativeTestLibrary` copies the specified binary into the test output directory.
Use the actual Cargo output path if `CARGO_TARGET_DIR` is set. On other platforms,
the library filename is `spacetimedb_test_datastore_ffi.dll` on Windows; platform
release validation is still pending. The adapter uses normal .NET native-library
resolution and checks the ABI version before creating a datastore.

Datastore and transaction handles own native resources through `SafeHandle`.
Their shared lock serializes calls and destruction, including finalizer cleanup.
Keep this synchronization in `NativeDatastore` and `NativeTransaction`, not in
`DatabaseBackend` or other managed fixture state. The one-execution-flow contract
includes disposal; concurrent use or disposal of the same fixture is unsupported.
Managed caches and transaction state therefore need no additional locks. Separate
fixtures remain independent and may run in parallel.
Disposing an unfinished transaction rolls it back. Disposing a datastore closes
outstanding transactions; their handles remain safe to dispose. Returned buffers
are copied before being freed by the native allocator, including on error paths.
