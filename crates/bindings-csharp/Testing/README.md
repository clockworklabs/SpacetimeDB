# Managed Module Testing

This .NET 10 project provides `TestContext.Create(typeof(Module).Assembly)`,
fixture-owned `Db`, and synchronous `WithReducerTx` callbacks backed by the native
test datastore. Use `TestAuth.Internal()` for the database identity, or
`TestAuth.FromJwtPayload(json, connectionId)` for a client. Client credentials
require a nonzero connection ID and string `iss`/`sub` claims of 1-128 UTF-8 bytes.
An optional `hex_identity` must match the derived identity. Other claims are
preserved for the existing `JwtClaims` API; signatures and expiry are not checked.
Identity derivation uses the production Rust implementation through the native
test bridge, without adding a hashing dependency to Runtime. Rebuild the bridge
when updating to this API. Credentials are reusable; parsed claims are per invocation.
Transactions commit on normal return and roll back on exceptions; their handles are invalid
after the callback finishes. Default clocks start at the Unix epoch, and RNGs
start at root seed zero. `Clock.Set`/`Advance` and `Rng.SetSeed`/`ClearSeed` control
subsequent contexts. Managed module logging writes to standard error.

`SetEnv(key, value)` and `UnsetEnv(key)` configure fixture-local environment values
declared by the root module. Keys are case-sensitive; values must satisfy declared
literal/union constraints and the engine's 8192-byte UTF-8 limit. Fixtures start
with no values. Required generated accessors throw for missing values; empty
strings remain present. Reads see current values, and transaction rollback does
not undo environment changes. Environment handles expire with their owning context.

`ViewContext(auth)` and `AnonymousViewContext()` create fixture-owned read-only
contexts for calling module views directly. `RunQuery<T>(IQuery<T>)` executes their
typed SELECT queries, including semijoins, against committed tables and returns
materialized rows. Row types must implement `IStructuralReadWrite` and have a
parameterless constructor, as generated module rows do. Queries execute with the
database identity; caller filtering comes from the view's `Sender` and query.
The native bridge rejects queries while a mutable transaction is active.
Contexts expire on fixture disposal; materialized results remain ordinary values.

`ProcedureContext(auth)` creates a context for calling procedures directly.
`WithTx` commits a returned value and rolls back/rethrows exceptions; `TryWithTx`
also rolls back a returned `Result.Err` and reports failures through `TxOutcome`.
There is no transaction around the whole procedure, so earlier commits survive
later failures. Each transaction gets fresh handles and a timestamp from the
fixture clock at entry. Auth and RNG are shared with the procedure, and environment
reads remain live. Escaped transaction handles never become valid again.
Use `tx.Db` inside the callback, not `procedure.Db`; nested transactions and
async/awaitable callbacks are rejected.

Add `new ProcedureTestHooks().AfterTxCommit(fixture => { ... })` with the builder's
`Hooks(...)` method to run a synchronous callback after each successful procedure
transaction has committed and closed. Hooks can access committed data and advance
the fixture clock for the next transaction. They do not run after rollback.
Hook exceptions propagate directly from both `WithTx` and `TryWithTx`, without
undoing the commit or being converted into `TxOutcome` failures. Built contexts
capture their hook; later changes to the hook configuration do not affect them.
Calling `AfterTxCommit` again replaces the configured callback. There is no sleep hook.

Use `ProcedureContextBuilder(auth).Http((fixture, request) => ...).Build()` to
install a per-context HTTP responder returning `Result<HttpResponse, HttpError>`.
It receives the existing SpacetimeDB request types, including the body. Non-2xx
responses remain `Ok`; transport failures are `Err`. Responder exceptions and
assertion failures propagate unchanged. Without a responder, HTTP returns
`no test HTTP responder configured` and never calls the network or host imports.
HTTP is rejected while any transaction on that fixture is active. Empty URIs
and negative timeouts are rejected, and timeouts above 180 seconds are clamped
without changing the caller's request. Built contexts retain their responder
even if the builder is later reconfigured; fixture disposal invalidates HTTP.

Use one execution flow per fixture. Async callbacks and multicast delegates are
rejected; returned `Task`/`ValueTask` values also cause rollback. Independent
fixtures may run in parallel. Table deletion and clearing use the owning
transaction, and the engine removes event rows on commit. Unique-index lookup,
update, and deletion use fixture-owned index resolution, including mounted modules.
Range filtering and deletion support the existing inclusive bounds and composite
prefixes, with owner-lifetime checks on iterators. Unsupported host services fail
explicitly; lifecycle and scheduled reducers must be invoked directly.

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
architecture; CI must build and execute them on each supported platform. macOS
is outside the initial release matrix.

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

The `csharp_testing_pack` GitHub Actions job assembles Testing with all three
tested Release bridges, verifies their packaged bytes, and uploads a single
`spacetimedb-testing-packages` artifact with matching Runtime and BSATN.Runtime
packages. A separate negative restore check treats NuGet's out-of-range dependency
warning (NU1608) as an error to verify Testing's exact Runtime dependency.

The `csharp_testing_packages` matrix downloads that same package set on every
supported platform. It copies the chat template module and sibling test project
outside the checkout, restores with fresh NuGet caches,
and runs `dotnet test`. SpacetimeDB packages must come from the downloaded feed;
test-framework dependencies come from NuGet.org. Temporary files are removed
when the step finishes. The Test Suite aggregate requires the matrix to pass.

The chat tests cover fixture creation, caller identity and time, reducer commit
and rollback, indexed lookup, and disposal. The matrix also creates a new xUnit
project with the documented `dotnet new`, `dotnet add`, and `dotnet test` commands
and runs the same tests. Both paths use the packaged analyzers and native library
without repository project references, manual native-library copying, or
`NativeTestLibrary`. The consumer workflow does not invoke Cargo or a WASI toolchain.
The `csharp_testing_native` matrix builds the Release bridge and runs the managed
integration suite natively on Linux x64, Linux ARM64, and Windows x64 before uploading
`spacetimedb-testing-native-<rid>` artifacts containing `<rid>/<native-library>`.
Linux builds use Ubuntu 22.04 (glibc 2.35); CI rejects newer glibc requirements.
Windows builds and tests use Windows Server 2022 with MSVC. These are the tested
OS baselines, not a claim of compatibility with older systems. musl and macOS
are outside this matrix. CI logs include native architecture and dependency reports.
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
