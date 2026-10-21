# Managed Testing Interop

This .NET 10 project contains the internal managed adapter for the native test
datastore. Public test contexts and runtime routing are not implemented yet.
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
Disposing an unfinished transaction rolls it back. Disposing a datastore closes
outstanding transactions; their handles remain safe to dispose. Returned buffers
are copied before being freed by the native allocator, including on error paths.
