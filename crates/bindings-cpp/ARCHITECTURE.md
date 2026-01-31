# SpacetimeDB C++ Bindings Architecture

## Overview

The SpacetimeDB C++ bindings provides a sophisticated compile-time/runtime hybrid system for building database modules in C++ that compile to WebAssembly (WASM) and run inside the SpacetimeDB database. This document describes the architectural components, type registration flow, and key differences from other language SDKs.

## Core Architecture Principles

### 1. Hybrid Compile-Time/Runtime System
- **Compile-time validation**: C++20 concepts and static assertions catch constraint violations before compilation
- **Runtime registration**: __preinit__ functions execute during WASM module load to register types and metadata
- **Nominal type system**: Types identified by their declared names, not structural analysis
- **Error detection**: Multi-layer validation system from compile-time through module publishing

### 2. Outcome<T> - Rust-Like Error Handling
The SDK provides `Outcome<T>`, a type-safe error handling mechanism matching Rust's `Result<T, E>` pattern:
- **Outcome<void>** (type alias `ReducerResult`): Used by reducers, can return success (`Ok()`) or error (`Err(message)`)
- **Outcome<T>**: Useful for methods used by Reducers to return a value or an error message
- **No exceptions**: Errors are handled via return values, not C++ exceptions
- **Graceful error handling**: Reducer errors are caught by the runtime, rolled back, and reported to the caller without crashing
- **Serializable**: Error messages are automatically serialized and sent to clients

### 2. Priority-Ordered Initialization System
The SDK uses a numbered __preinit__ function system to ensure correct initialization order:

```
__preinit__01_ - Clear global state (first)
__preinit__10_ - Field registration
__preinit__19_ - Auto-increment integration and scheduled reducers
__preinit__20_ - Table and lifecycle reducer registration
__preinit__21_ - Field constraints
__preinit__25_ - Row level security filters
__preinit__30_ - User reducers
__preinit__40_ - Views
__preinit__50_ - Procedures
__preinit__99_ - Type validation and error detection (last)
```

## Error Handling: Outcome<T> System

### Overview

The C++ bindings uses `Outcome<T>` for type-safe, exception-free error handling that matches Rust's `Result<T, E>` pattern (where `E` is always `std::string`).

### Type Aliases and Core Types

```cpp
// For reducers - cannot fail with a value, only with an error message
using ReducerResult = Outcome<void>;
```

### Reducer Error Handling (ReducerResult / Outcome<void>)

**Creating Results**:
```cpp
#include <spacetimedb.h>

using namespace SpacetimeDB;

struct User {
    Identity identity;
    std::optional<std::string> name;
    bool online;
};
SPACETIMEDB_STRUCT(User, identity, name, online);
SPACETIMEDB_TABLE(User, user, Public);
FIELD_PrimaryKey(user, identity);


SPACETIMEDB_REDUCER(create_user, ReducerContext ctx, std::string name) {
    // Validation with early error return
    if (name.empty()) {
        return Err("Name cannot be empty");
    }
    if (name.length() > 255) {
        return Err("Name is too long");
    }
    
    // Success path
    ctx.db[user].insert(User{ctx.sender, name, false});
    return Ok();  // No value needed - just success
}
```

**Checking Results**:
```cpp
SPACETIMEDB_REDUCER(call_other_logic, ReducerContext ctx) {
    // Note: In practice, reducers don't call other reducers directly
    // But if implementing error-handling helper functions:
    auto result = validate_something();
    
    if (result.is_err()) {
        return Err(result.error());  // Propagate error
    }
    
    // Continue on success
    return Ok();
}
```

**Error Semantics**:
- When `Err()` is returned:
  - The reducer transaction is **rolled back** (not committed to the log)
  - The error message is captured and returned to the caller
  - No database changes are persisted
  - No WASM crash or panic occurs
- When `Ok()` is returned:
  - All database mutations are committed
  - The transaction is logged
  - Success is reported to the caller

### Procedure Error Handling

**Key Difference from Reducers**:
- Procedures return raw `T` (not `Outcome<T>`)
- On error, procedures can use LOG_PANIC() or LOG_FATAL() to end the host call which uses std:abort() behind the scenes
- Return values are sent directly to the caller

### Outcome<T> API Reference

```cpp
// Creating success outcomes
Outcome<T>::Ok(value)      // Outcome<T> - with a value
Ok()                       // Outcome<void> - without a value  
Ok(value)                  // Helper - type deduced from value

// Creating error outcomes
Outcome<T>::Err(message)   // Outcome<T> - with error message
Err(message)               // Outcome<void> - with error message
Err<T>(message)            // Helper - explicit type specification

// Checking results
outcome.is_ok()            // bool - true if success
outcome.is_err()           // bool - true if error

// Accessing values/errors
outcome.value()            // T& or T&& - get success value (UB if error)
outcome.error()            // const std::string& - get error message (UB if success)
```

### Design Rationale

**Why not exceptions?**
- WASM modules have limited error handling facilities, the latest WASM allows for them but requires GC
- Exceptions add code size and complexity
- Explicit error returns fit better with BSATN serialization
- Matches Rust SDK's error handling pattern

**Why separate ReducerResult and Outcome<T>?**
- Reducers need rollback semantics (transactions)
- ReducerResult provides clearer intent for reducer code
- Outcome<T> is more flexible for general operations

---

## Detailed Type Registration Flow

### Phase 1: Compile-Time Validation

**Location**: Template instantiation during compilation

**Components**:
- **C++20 Concepts** (`table_with_constraints.h`):
  ```cpp
  template<typename T>
  concept FilterableValue = 
      std::integral<T> ||
      std::same_as<T, std::string> ||
      std::same_as<T, Identity> ||
      // ... other filterable types
  
  template<typename T>
  concept AutoIncrementable = 
      std::same_as<T, int8_t> ||
      std::same_as<T, uint32_t> ||
      // ... integer types only
  ```

- **Static Assertions** in FIELD_ macros:
  ```cpp
  #define FIELD_Unique(table_name, field_name) \
      static_assert([]() constexpr { \
          using FieldType = decltype(std::declval<TableType>().field_name); \
          static_assert(FilterableValue<FieldType>, \
              "Field cannot have Unique constraint - type is not filterable."); \
          return true; \
      }(), "Constraint validation for " #table_name "." #field_name);
  ```

**Validation Coverage**:
- ✅ AutoIncrement constraints (only integer types)
- ✅ Index/Unique/PrimaryKey constraints (only filterable types)
- ✅ Type compatibility with BSATN serialization
- ✅ Template parameter validation

**Error Output**: Clear compile-time error messages with specific guidance

### Phase 2: Runtime Registration (__preinit__ functions)

**Location**: WASM module load, before any user code executes

#### 2.1 Global State Initialization (__preinit__01_)
```cpp
extern "C" __attribute__((export_name("__preinit__01_clear_global_state")))
void __preinit__01_clear_global_state() {
    ClearV9Module();  // Reset module definition and handler registries
    getV9TypeRegistration().clear();  // Reset type registry and error state
}
```

#### 2.2 Component Registration (__preinit__10-30_)
Generated by macros during preprocessing:

**Table Registration** (__preinit__20_):
```cpp
SPACETIMEDB_TABLE(User, users, Public)
// Generates:
extern "C" __attribute__((export_name("__preinit__20_register_table_User_line_42")))
void __preinit__20_register_table_User_line_42() {
    SpacetimeDB::Module::RegisterTable<User>("users", true);
}
```

**Field Constraints** (__preinit__21_):
```cpp
FIELD_PrimaryKey(users, id);
// Generates:
extern "C" __attribute__((export_name("__preinit__21_field_constraint_users_id_line_43")))
void __preinit__21_field_constraint_users_id_line_43() {
    getV9Builder().AddFieldConstraint<User>("users", "id", FieldConstraint::PrimaryKey);
}
```

**Auto-Increment Integration Registration** (__preinit__19_):
Auto-increment fields require special handling during `insert()` operations. When SpacetimeDB processes an auto-increment insert, it returns only the generated column values (not the full row) in BSATN format. The C++ bindings uses a registry-based integration system to properly handle these generated values and update the user's row object.

```cpp
FIELD_PrimaryKeyAutoInc(users, id);
// Generates both constraint registration AND auto-increment integration:

// 1. Auto-increment integration function (unique per field via __LINE__)
namespace SpacetimeDB { namespace detail {
    static void autoinc_integrate_47(User& row, SpacetimeDB::bsatn::Reader& reader) {
        using FieldType = decltype(std::declval<User>().id);
        FieldType generated_value = SpacetimeDB::bsatn::deserialize<FieldType>(reader);
        row.id = generated_value;  // Update field with generated ID
    }
}}

// 2. Registration function to register the integrator
extern "C" __attribute__((export_name("__preinit__19_autoinc_register_47")))
void __preinit__19_autoinc_register_47() {
    SpacetimeDB::detail::get_autoinc_integrator<User>() = 
        &SpacetimeDB::detail::autoinc_integrate_47;
}
```

**Runtime Integration Process**:
When `insert()` is called on a table with auto-increment fields:
1. The logic in the bindings serializes and sends the row to SpacetimeDB
2. SpacetimeDB processes the insert and generates the auto-increment value(s)
3. SpacetimeDB returns a buffer containing only the generated column values in BSATN format
4. SDK calls the registered integrator function to update the original row with generated values
5. `insert()` returns the updated row with the correct generated ID

This system enables users to immediately access generated IDs:
```cpp
struct User {
    uint64_t id;
    std::optional<std::string> name;
};
SPACETIMEDB_STRUCT(User, id, name);
SPACETIMEDB_TABLE(User, user, Public);
FIELD_PrimaryKeyAutoInc(user, id);

SPACETIMEDB_REDUCER(create_user2, ReducerContext ctx, std::string name) {
    User new_user{0, name};  // id=0 will be auto-generated
    User inserted_user = ctx.db[user].insert(new_user);  // Returns user with generated ID
    LOG_INFO("Created user with ID: " + std::to_string(inserted_user.id));
    return Ok();  // Must return ReducerResult
}
```

**Reducer Registration** (__preinit__30_):
```cpp
SPACETIMEDB_REDUCER(add_user, ReducerContext ctx, std::string name) {
    if (name.empty()) {
        return Err("Name cannot be empty");  // Return error - rolled back
    }
    ctx.db[user].insert(User{0, name});
    return Ok();  // Success - transaction committed
}
// Generates registration function that captures parameter types, creates dispatch handler,
// and wraps return value in ReducerResult (Outcome<void>)
```

#### 2.3 Multiple Primary Key Detection
During constraint registration, track primary keys per table:
```cpp
// In V9Builder::AddFieldConstraint
if (constraint == FieldConstraint::PrimaryKey) {
    if (table_has_primary_key[table_name]) {
        SetMultiplePrimaryKeyError(table_name);  // Set global error flag
    }
    table_has_primary_key[table_name] = true;
}
```

### Phase 3: Type System Registration

**Component**: V9TypeRegistration system (`v9_type_registration.h`)

**Core Principle**: Only user-defined structs and enums get registered in the typespace. Primitives, arrays, Options, and special types are always inlined.

**Architecture Note**: V9Builder serves as the registration coordinator but delegates all type processing to the V9TypeRegistration system. This separation ensures a single, unified type registration pathway.

**Registration Flow**:
```cpp
class V9TypeRegistration {
    AlgebraicType registerType(const bsatn::AlgebraicType& bsatn_type,
                              const std::string& explicit_name = "",
                              const std::type_info* cpp_type = nullptr) {
        // 1. Check if primitive → return inline
        if (isPrimitive(bsatn_type)) return convertPrimitive(bsatn_type);
        
        // 2. Check if array → return inline Array with recursive element processing
        if (bsatn_type.tag() == bsatn::AlgebraicTypeTag::Array) 
            return convertArray(bsatn_type);
        
        // 3. Check if Option → return inline Sum structure
        if (isOptionType(bsatn_type)) return convertOption(bsatn_type);
        
        // 4. Check if special type → return inline Product structure
        if (isSpecialType(bsatn_type)) return convertSpecialType(bsatn_type);
        
        // 5. User-defined type → register in typespace, return Ref
        return registerUserDefinedType(bsatn_type, explicit_name, cpp_type);
    }
};
```

**Circular Reference Detection**:
```cpp
// Track types currently being registered
std::unordered_set<std::string> types_being_registered_;

AlgebraicType registerUserDefinedType(...) {
    if (types_being_registered_.contains(type_name)) {
        setError("Circular reference detected in type: " + type_name);
        return createErrorType();
    }
    types_being_registered_.insert(type_name);
    // ... process type ...
    types_being_registered_.erase(type_name);
}
```

### Phase 4: Validation and Error Detection (__preinit__99_)

**Location**: Final preinit function - runs after all registration is complete

**Error Detection**:
```cpp
extern "C" __attribute__((export_name("__preinit__99_validate_types")))
void __preinit__99_validate_types() {
    // 1. Check for circular reference errors
    if (g_circular_ref_error) {
        createErrorModule("ERROR_CIRCULAR_REFERENCE_" + g_circular_ref_type_name);
        return;
    }
    
    // 2. Check for multiple primary key errors
    if (g_multiple_primary_key_error) {
        createErrorModule("ERROR_MULTIPLE_PRIMARY_KEYS_" + g_multiple_primary_key_table_name);
        return;
    }
    
    // 3. Check for type registration errors
    if (getV9TypeRegistration().hasError()) {
        createErrorModule("ERROR_TYPE_REGISTRATION_" + sanitize(error_message));
        return;
    }
}
```

**Error Module Creation**: When errors are detected, the normal module is replaced with a special error module containing an invalid type reference. When SpacetimeDB tries to resolve the type, it fails with an error message that includes the descriptive error type name.

### Phase 5: Module Description Export

**Function**: `__describe_module__()` - Called by SpacetimeDB after preinit functions complete

**Process**:
1. Serialize the completed V9 module definition
2. Include typespace (all registered types)
3. Include tables with constraints
4. Include reducers with parameter types
5. Include named type exports
6. Return binary module description

## Namespace Qualification System

### Overview
The C++ bindings provides a unique compile-time namespace qualification system for enum types, allowing better organization in generated client code without affecting server-side C++ usage.

### Architecture Components

#### 1. Compile-Time Namespace Storage
**Location**: `enum_macro.h` - namespace_info template specialization

```cpp
namespace SpacetimeDB::detail {
    // Primary template - no namespace by default
    template<typename T>
    struct namespace_info {
        static constexpr const char* value = nullptr;
    };
}

// SPACETIMEDB_NAMESPACE macro creates specialization
#define SPACETIMEDB_NAMESPACE(EnumType, NamespacePrefix) \
    namespace SpacetimeDB::detail { \
        template<> \
        struct namespace_info<EnumType> { \
            static constexpr const char* value = NamespacePrefix; \
        }; \
    }
```

#### 2. LazyTypeRegistrar Integration
**Location**: `v9_type_registration.h` - Compile-time namespace detection

```cpp
template<typename T>
class LazyTypeRegistrar {
    static bsatn::AlgebraicType getOrRegister(...) {
        std::string qualified_name = type_name;
        
        // Compile-time check for namespace information
        if constexpr (requires { SpacetimeDB::detail::namespace_info<T>::value; }) {
            constexpr const char* namespace_prefix = 
                SpacetimeDB::detail::namespace_info<T>::value;
            if (namespace_prefix != nullptr) {
                qualified_name = std::string(namespace_prefix) + "." + type_name;
            }
        }
        
        // Register with qualified name
        type_index_ = getV9TypeRegistration().registerAndGetIndex(
            algebraic_type, qualified_name, &typeid(T));
    }
};
```

#### 3. Type Registration with Namespaces
When an enum with namespace qualification is registered:
1. SPACETIMEDB_ENUM defines the enum and its BSATN traits
2. SPACETIMEDB_NAMESPACE adds compile-time metadata
3. LazyTypeRegistrar detects namespace at compile-time
4. Type is registered with qualified name (e.g., "Auth.UserRole")
5. Client generators recognize the namespace structure

### Design Rationale

**Why Separate Macros?**
- Clean separation of concerns: enum definition vs. namespace qualification
- Optional feature - enums work without namespaces
- Non-intrusive - doesn't modify the enum type itself
- Compile-time only - zero runtime overhead

**Why Template Specialization?**
- Type-safe association between enum and namespace
- Compile-time resolution - no runtime lookups
- Works with C++20 concepts and if constexpr
- No memory overhead - constexpr strings

### Comparison with Other Approaches

**Alternative 1: Preinit Runtime Modification** (Rejected)
- Would require modifying types after registration
- Complex synchronization with type registry
- Runtime overhead for namespace lookup

**Alternative 2: Embedded in SPACETIMEDB_ENUM** (Rejected)
- Would complicate the macro syntax
- Makes namespace mandatory rather than optional
- Harder to add namespaces to existing code

**Current Approach Benefits**:
- Clean, modular design
- Zero runtime cost
- Optional and backwards-compatible
- Easy to understand and maintain

## Key Differences from Rust and C# SDKs

### 1. Type Registration Approach

**Rust bindings**:
- Derive macros automatically generate type registration code
- Compile-time code generation using procedural macros
- Direct integration with Rust's type system
- Option types automatically inlined by macro system

**C# bindings**:
- Reflection-based runtime type discovery
- Attribute-based configuration
- Dynamic type registration during module initialization
- .NET type system integration

**C++ bindings**:
- Template-based compile-time validation with runtime registration
- Macro-generated __preinit__ functions for ordered initialization
- Manual type registration via SPACETIMEDB_STRUCT macros
- Hybrid approach combining compile-time safety with runtime flexibility

### 2. Constraint Validation

**Rust bindings**:
- Procedural macros generate compile-time validation
- Type system automatically enforces valid constraints
- No runtime constraint checking needed

**C# bindings**:
- Runtime validation using reflection
- Attributes specify constraints, validated during registration
- Dynamic error reporting

**C++ bindings**:
- **Three-layer validation system**:
  1. **Compile-time**: C++20 concepts and static assertions
  2. **Registration-time**: Multiple primary key detection
  3. **Module load**: preinit_99_ comprehensive validation
- Most sophisticated error detection of all SDKs

### 3. Error Handling Strategy

**Rust bindings**:
- Result<T, E> for operation errors with rich error types
- Compile-time errors prevent building invalid modules
- Type system prevents most runtime errors
- Standard Rust error messages

**C# bindings**:
- Runtime exceptions with detailed error messages
- Graceful error handling with exception propagation
- .NET debugging tools integration

**C++ bindings** - Two-Tier System:
1. **Reducer errors** (ReducerResult / Outcome<void>):
   - Return `Ok()` on success (transaction committed)
   - Return `Err(message)` on failure (transaction rolled back)
   - Exceptions not used for normal error cases
   - Matches Rust's Result<(), E> pattern
   
2. **Type registration errors**:
   - Invalid modules replaced with special error modules
   - Error type names embed descriptive information
   - SpacetimeDB server provides clear error messages
   - Comprehensive error categorization and reporting

**Outcome<T> Type**:
- Type-safe, exception-free error handling
- Serializable to binary format for client transmission
- Works in WASM environment without exception infrastructure
- API matches Rust Result pattern: `is_ok()`, `is_err()`, `value()`, `error()`

### 4. Type System Philosophy

**Rust bindings**: 
- "If it compiles, it works" - maximum compile-time validation
- Leverages Rust's ownership and type system
- Minimal runtime overhead

**C# bindings**:
- "Flexibility with safety" - runtime validation with rich error messages
- Leverages .NET reflection and attributes
- Dynamic type discovery

**C++ bindings**:
- **"Validate early, validate often"** - multi-layer validation system
- Combines C++20 compile-time features with runtime checks
- Nominal type system with explicit registration
- Optimized for catching errors at the earliest possible phase

## Memory Management and Performance

### Compile-Time Optimizations
- Template specialization eliminates runtime overhead
- Constexpr evaluations reduce WASM binary size
- Zero-cost abstractions for type-safe database access

### Runtime Efficiency
- Minimal allocation during type registration
- Efficient binary serialization with BSATN
- Optimized field accessors with index caching

### WASM Constraints
- 16MB initial memory limit (configurable)
- No dynamic memory growth during module registration
- Careful memory management in preinit functions

## Development Workflow Integration

### Error Detection Timeline
```
Developer writes code
    ↓
C++ compilation → Compile-time validation (concepts, static_assert)
    ↓
Emscripten WASM build → Template instantiation validation
    ↓
Module publishing → Runtime validation (__preinit__99_)
    ↓
SpacetimeDB loading → Server-side validation and error reporting
```

### Debugging Support
- **Compile-time**: Clear error messages with field/constraint guidance
- **Build-time**: Template instantiation error reporting
- **Runtime**: Comprehensive logging with error categorization
- **Server-side**: Descriptive error module names for easy diagnosis

## Future Architecture Considerations

### Potential Improvements
1. **Unified Validation**: Move more validation to compile-time using concepts
2. **Better Error Recovery**: Partial module loading with isolated error handling
3. **Performance Optimization**: Reduce template instantiation overhead
4. **Enhanced Debugging**: Source location tracking for runtime errors

### Scalability
- Type registration system scales linearly with module complexity
- Preinit function count grows with table/reducer count but remains manageable
- Memory usage is predictable and bounded

## Related Documentation

- [Type System Details](README.md#features)
- [Constraint Validation Tests](tests/type-isolation-test/)
- [API Reference](REFERENCE.md)
- [Quick Start Guide](QUICKSTART.md)