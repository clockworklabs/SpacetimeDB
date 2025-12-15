# BSATN C++ Library

This directory contains a self-contained C++ implementation of BSATN (Binary SpacetimeDB Algebraic Type Notation) serialization.

## For C++ Client Usage

The BSATN library can be used independently for serialization/deserialization without any SpacetimeDB module dependencies. 

### What you need:
- All headers in this directory
- Standard C++ library (C++20)
- No external dependencies

### What you DON'T need:
- `ITypeRegistrar.h` - This is only for modules, not clients
- Type registration functionality
- Any files from outside this directory

### Basic usage:

```cpp
#include "bsatn/bsatn.h"

// Define your struct
struct MyData {
    uint32_t id;
    std::string name;
};

// Define serialization traits
SPACETIMEDB_STRUCT(MyData, id, name)

// Serialize
MyData data{42, "example"};
std::vector<uint8_t> buffer;
SpacetimeDb::bsatn::Writer writer(buffer);
SpacetimeDb::bsatn::serialize(writer, data);

// Deserialize
SpacetimeDb::bsatn::Reader reader(buffer);
auto result = SpacetimeDb::bsatn::deserialize<MyData>(reader);
```

## Architecture Notes

- **ITypeRegistrar.h**: Interface for optional type registration. Clients can ignore this - it's only used by SpacetimeDB modules. The interface is kept here to avoid circular dependencies while maintaining clean architecture.

- **No external dependencies**: All files only include other BSATN headers or standard C++ library headers.

- **Special types**: The library includes SpacetimeDB special types (Identity, ConnectionId, Timestamp, TimeDuration) that serialize with specific tags for compatibility.

## File Structure

- Core: `reader.h`, `writer.h`, `serialization.h`
- Type system: `algebraic_type.h`, `traits.h`, `primitive_traits.h`
- Special types: `types.h`, `timestamp.h`, `time_duration.h`, `special_types.h`
- Utilities: `size_calculator.h`, `sum_type.h`
- Module-only: `ITypeRegistrar.h` (can be ignored by clients)