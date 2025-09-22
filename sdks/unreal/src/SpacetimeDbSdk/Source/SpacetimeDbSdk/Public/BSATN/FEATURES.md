# Unreal BSATN Package Features

This document lists the main capabilities provided by the Unreal BSATN
integration.

## Supported Types

### Complete Type Support
- Primitive integers and floats
- `FString` and `FName`
- `TArray<T>` and `TOptional<T>`
- `FVector`, `FRotator`, `FTransform`
- `FGuid`, `FDateTime`, `FTimespan`
- SpacetimeDB specific types (see `UNREAL_BSATN_ADDITIONS.md`)

## API Highlights
- `UE_SPACETIMEDB_STRUCT` &ndash; enable struct serialization (up to 10 fields)
- `UE_SPACETIMEDB_ENABLE_TARRAY(T)` &ndash; allow `TArray` of custom types
- `UE_SPACETIMEDB_ENABLE_TOPTIONAL(T)` &ndash; allow `TOptional` of custom types
- `Serialize(value)` / `Deserialize<T>(bytes)` convenience helpers

The implementation is header&nbsp;only and can be compiled outside Unreal using
`MockCoreMinimal.h`.