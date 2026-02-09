# SDK Test Procedure C++

This module tests the procedure functionality in the SpacetimeDB C++ bindings.

## Tests Included

- **return_primitive**: Procedure returning primitive type (u32)
- **return_struct**: Procedure returning custom struct
- **return_enum_a/return_enum_b**: Procedures returning enum variants
- **will_panic**: Procedure that panics (for error testing)

## Tests Excluded (Part 2+)

The following Rust tests are excluded as they require features not yet implemented:
- HTTP requests (`read_my_schema`, `invalid_request`)
- Transactions (`insert_with_tx_commit`, `insert_with_tx_rollback`)
- Scheduled procedures (`schedule_proc`, `scheduled_proc`)

## Building

```bash
.\compile.bat
```

This will generate `lib.wasm` in the build directory.
