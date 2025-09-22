# Unreal Engine BSATN Client Additions

Additional helpers extend the core BSATN library so the Unreal client can
serialize every data type used by SpacetimeDB.

## Special Types

- `Identity`, `ConnectionId`, `Timestamp` and `TimeDuration`
- 128&nbsp;bit and 256&nbsp;bit integer wrappers

## Sum Types and Enums

- `TSumType` template for discriminated unions
- `UE_SPACETIMEDB_ENUM` macro for enum serialization

## Container Support

- Arrays and optionals of the above types

## Implementation Notes

- No runtime type registration is required
- Binary layout matches the server implementation
- All multi-byte values use little-endian encoding