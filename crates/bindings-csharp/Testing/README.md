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

Use one execution flow per fixture. Async callbacks and multicast delegates are
rejected; returned `Task`/`ValueTask` values also cause rollback. Independent
fixtures may run in parallel. Table deletion and clearing use the owning
transaction, and the engine removes event rows on commit. Unique-index lookup,
update, and deletion use fixture-owned index resolution, including mounted modules.
Range filtering and deletion support the existing inclusive bounds and composite
prefixes, with owner-lifetime checks on iterators. Views, procedures,
and other host services are still pending.

NuGet packaging and native asset distribution are deferred; this project is
currently not packable.

From the repository root, build the native bridge and run the integration tests
(Linux example):

```sh
cargo build -p spacetimedb-test-datastore-ffi
dotnet test crates/bindings-csharp/Testing.Tests/Testing.Tests.csproj \
  -p:NativeTestLibrary="$PWD/target/debug/libspacetimedb_test_datastore_ffi.so"
```

`NativeTestLibrary` copies the specified binary into the test output directory.
Use the actual Cargo output path if `CARGO_TARGET_DIR` is set. On other platforms,
the library filename is `libspacetimedb_test_datastore_ffi.dylib` (macOS) or
`spacetimedb_test_datastore_ffi.dll` (Windows); those platforms still require
validation. The adapter uses normal .NET native-library resolution and checks
the ABI version before creating a datastore.

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
