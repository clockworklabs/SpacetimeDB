# BSATN

This directory ships the header‑only implementation of SpacetimeDB's BSATN
serializer used by the Unreal client SDK.  The code mirrors the server side
library and enables serializing common Unreal Engine types to the BSATN wire
format.

## Contents

- `Core/` – Cross‑platform serialization library copied from
  `crates/bindings-cpp/include/spacetimedb/bsatn`.
- `MockCoreMinimal.h` – Minimal stand‑ins for a handful of engine types when
  compiling outside of Unreal.
- `UEBSATNHelpers.h` – Helper macros for registering UE types and containers.
- `UESpacetimeDB.h` – Umbrella header that exposes `Serialize` and `Deserialize`
  functions and the `UE_SPACETIMEDB_STRUCT` macro.
- `FEATURES.md` – Summary of supported types.
- `UNREAL_BSATN_ADDITIONS.md` – Extra serialization helpers for the client.
- 
## Usage

Include `UESpacetimeDB.h` in your project and call the helper functions:

```cpp
TArray<uint8> Bytes = UE::SpacetimeDB::Serialize(MyStruct);
MyStruct Restored = UE::SpacetimeDB::Deserialize<MyStruct>(Bytes);
```

Custom structs opt in via `UE_SPACETIMEDB_STRUCT`.  The library has no runtime
dependencies and consists solely of header files.


## Implementation Notes

- No runtime type registration is required
- Binary layout matches the server implementation
- All multi-byte values use little-endian encoding