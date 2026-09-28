# sdk-test-view-pk-cpp

C++ implementation of the SpacetimeDB query-builder view PK test module.
This mirrors the Rust [`sdk-test-view-pk`](/D:/ClockworkLabs/branches/SpacetimeDB/modules/sdk-test-view-pk/src/lib.rs)
and C# [`sdk-test-view-pk-cs`](/D:/ClockworkLabs/branches/SpacetimeDB/modules/sdk-test-view-pk-cs/Lib.cs) modules.

## Coverage

- query-returning views over base tables
- PK-preserving query views
- right semijoins from indexed membership tables to player rows

## Views

- `all_view_pk_players`
- `sender_view_pk_players_a`
- `sender_view_pk_players_b`

## Build

```powershell
.\modules\sdk-test-view-pk-cpp\compile.bat
```

The output will be `modules/sdk-test-view-pk-cpp/build/lib.wasm`.
