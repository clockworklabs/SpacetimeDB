# C++ Bindings Development Roadmap: Leveraging Modern C++ Standards

This document explores how upgrading to C++23 and C++26 could fundamentally transform the SpacetimeDB C++ bindings, moving from runtime registration to compile-time type system integration and eliminating most macro usage.

## Current Architecture: Why So Many Macros?

### The Fundamental Problem: No Compile-Time Reflection in C++20

Without reflection, the compiler cannot:
- Enumerate struct fields automatically
- Determine field types and names
- Discover which types should be tables
- Generate serialization code
- Build type relationships

This forces us into a multi-layer macro system with runtime registration:

### Layer 1: SPACETIMEDB_STRUCT - Manual Field Enumeration

```cpp
struct User {
    uint32_t id;
    std::string name;
    std::string email;
};
SPACETIMEDB_STRUCT(User, id, name, email)
```

**Why we need it:**
- C++ has no way to iterate over struct fields at compile-time
- We must manually list every field for serialization
- The macro generates a `bsatn_traits<User>` specialization that knows how to serialize/deserialize

**What it generates:**
```cpp
template<> struct bsatn_traits<User> {
    static AlgebraicType algebraic_type() {
        // Builds Product type with fields [id, name, email]
    }
    static void serialize(Writer& w, const User& val) {
        w.write(val.id); w.write(val.name); w.write(val.email);
    }
};
```

**Why it can't be compile-time:** We can't discover the fields without listing them explicitly.

### Layer 2: SPACETIMEDB_TABLE - Runtime Table Registration

```cpp
SPACETIMEDB_TABLE(User, users, Public)
```

**Why we need it separately from STRUCT:**
- A struct might be used as a table, a reducer parameter, or a nested type
- Not all structs are tables - some are just data types
- Table registration needs additional metadata (name, access level)

**What it generates:**
```cpp
extern "C" __attribute__((export_name("__preinit__20_register_table_User")))
void __preinit__20_register_table_User() {
    Module::RegisterTable<User>("users", true);
}
// Plus a tag type for clean syntax: ctx.db[users]
```

**Why it must be runtime:** The module schema must be built dynamically when the WASM module loads, as we can't generate a complete module description at compile-time.

### Layer 3: FIELD_ Macros - Constraint Registration

```cpp
FIELD_PrimaryKey(users, id);
FIELD_Unique(users, email);
FIELD_Index(users, name);
```

**Why we need separate macros per constraint:**
- Each field can have multiple constraints
- Constraints must be registered AFTER the table (priority ordering)
- Some constraints need compile-time validation (C++20 concepts)
- Can't be part of SPACETIMEDB_TABLE because we need the table to exist first

**What they generate:**
```cpp
// Compile-time validation
static_assert(FilterableValue<decltype(User::id)>, "Primary keys must be filterable");

// Runtime registration
extern "C" __attribute__((export_name("__preinit__21_field_constraint_users_id")))
void __preinit__21_field_constraint_users_id() {
    AddFieldConstraint<User>("users", "id", FieldConstraint::PrimaryKey);
}
```

**Why the split:** Compile-time validation via concepts, but runtime registration for module schema.

### Layer 4: __preinit__ Priority System - Ordered Initialization

```cpp
__preinit__01_ - Clear global state
__preinit__10_ - Field registration  
__preinit__19_ - Auto-increment integration and scheduled reducers
__preinit__20_ - Table and lifecycle reducer registration
__preinit__21_ - Field constraints (must come after tables)
__preinit__25_ - Row level security filters
__preinit__30_ - User reducers
__preinit__40_ - Views
__preinit__50_ - Procedures
__preinit__99_ - Type validation and error detection
```

**Why we need priority ordering:**
- Tables must exist before constraints can be added
- Types must be registered before they're referenced
- Validation must happen after everything else
- WebAssembly's linear memory model requires deterministic initialization

**Why it's runtime:** WASM modules are initialized linearly, and we need to build the module schema during this initialization phase.

### Layer 5: Namespace Qualification - Compile-Time Metadata

```cpp
SPACETIMEDB_ENUM(UserRole, Admin, Moderator, Member)
SPACETIMEDB_NAMESPACE(UserRole, "Auth")  // Separate macro for namespace
```

**Why we need a separate macro:**
- Namespace is optional metadata, not core to the enum definition
- Must work with existing enum definitions without modification
- Needs to associate compile-time string with type

**What it generates:**
```cpp
namespace SpacetimeDB::detail {
    template<> struct namespace_info<UserRole> {
        static constexpr const char* value = "Auth";
    };
}
```

**Why it's compile-time but still needs a macro:**
- C++20 has no way to attach metadata to types without explicit specialization
- Template specialization requires knowing the type name
- LazyTypeRegistrar uses `if constexpr` to detect namespace at compile-time

### Layer 6: __describe_module__ - Final Runtime Assembly

```cpp
extern "C" const uint8_t* __describe_module__() {
    // Serialize the complete module built by __preinit__ functions
    return Module::serialize();
}
```

**Why it's needed:**
- SpacetimeDB server calls this to get the module schema
- Must return a binary description of all tables, types, reducers
- Can only be built after all __preinit__ functions have run

**The fundamental limitation:** Without compile-time reflection, we cannot know at compile-time:
- Which types are tables
- What fields each struct has
- What constraints apply to each field
- The complete type dependency graph
- What namespace qualifications are applied

### The Cascade Effect

This creates a cascade of limitations:

1. **No automatic serialization** → Need SPACETIMEDB_STRUCT macro
2. **No type discovery** → Need explicit SPACETIMEDB_TABLE macro  
3. **No field introspection** → Need separate FIELD_ macros
4. **No compile-time module generation** → Need runtime __preinit__ system
5. **No static validation** → Need runtime validation in __preinit__99

Each macro exists because C++20 lacks the reflection capabilities to do this work automatically. The runtime registration exists because we can't build a complete module description at compile-time without knowing what types exist and their relationships.

## Current Architecture Limitations (Summary)

The current C++20 SDK relies on:
- **Runtime registration** via `__preinit__` functions (because no compile-time type discovery)
- **Heavy macro usage** for type and table registration (because no reflection)
- **Runtime error detection** in `__preinit__99_validate_types` (because incomplete compile-time info)
- **Manual serialization** through SPACETIMEDB_STRUCT macros (because no field enumeration)
- **String-based type identification** requiring explicit registration (because no compile-time type identity)
- **Outcome<void> error handling** for reducer transactions (workaround for lack of first-class error types)

## C++23 Improvements

### 1. Deducing `this` for Zero-Cost Field Accessors

**Current approach:**
```cpp
template<typename TableType, typename FieldType>
class TypedFieldAccessor : public TableAccessor<TableType> {
    FieldType TableType::*member_ptr_;
    // Complex inheritance hierarchy
};
```

**C++23 approach:**
```cpp
struct TableAccessor {
    template<typename Self>
    auto filter(this Self&& self, auto&& predicate) {
        // Deduce table type from self, no inheritance needed
        return self.table_.filter(std::forward<decltype(predicate)>(predicate));
    }
};
```

**Benefits:**
- Eliminate accessor class hierarchy
- Perfect forwarding throughout
- Reduced template instantiation overhead

### 2. `if consteval` for Hybrid Compile/Runtime Validation

**Current approach:**
```cpp
// Static assertions in macros
static_assert(FilterableValue<FieldType>, "Error message");
// Plus runtime validation in __preinit__99
```

**C++23 approach:**
```cpp
template<typename T>
constexpr auto validate_constraint() {
    if consteval {
        // Compile-time path: full validation
        static_assert(FilterableValue<T>);
        return compile_time_type_id<T>();
    } else {
        // Runtime fallback for dynamic types
        return runtime_type_registry::get<T>();
    }
}
```

**Benefits:**
- Single validation function works at compile-time or runtime
- Better error messages with source locations
- Gradual migration path from runtime to compile-time

### 3. `std::expected` for Error Propagation

**Current approach:**
```cpp
// Global error flags
static bool g_multiple_primary_key_error = false;
static std::string g_multiple_primary_key_table_name = "";
```

**C++23 approach:**
```cpp
template<typename T>
using RegistrationResult = std::expected<TypeId, RegistrationError>;

constexpr RegistrationResult register_table() {
    if (/* multiple primary keys detected */)
        return std::unexpected(RegistrationError::MultiplePrimaryKeys);
    return TypeId{...};
}
```

**Benefits:**
- Type-safe error handling
- Composable error propagation
- No global state needed

### 4. `constexpr std::unique_ptr` for Compile-Time Type Trees

**Current approach:**
```cpp
// Runtime type tree building
std::vector<AlgebraicType> types;
types.push_back(...);
```

**C++23 approach:**
```cpp
constexpr auto build_type_tree() {
    std::unique_ptr<TypeNode> root = std::make_unique<TypeNode>();
    // Build entire type hierarchy at compile time
    return root;
}

constexpr auto type_tree = build_type_tree();
```

**Benefits:**
- Complete type system known at compile-time
- Zero runtime allocation
- Enables compile-time validation of entire module

### 5. `std::ranges` for Cleaner Type Processing

**Current approach:**
```cpp
// Manual iteration and filtering
std::vector<AlgebraicType> types;
for (const auto& type : all_types) {
    if (isPrimitive(type)) continue;
    if (isOptionType(type)) continue;
    types.push_back(processType(type));
}
```

**C++23 approach:**
```cpp
// Declarative pipeline with ranges
auto process_types() {
    return all_types 
        | std::views::filter(not_primitive)
        | std::views::filter(not_option)
        | std::views::transform(processType)
        | std::ranges::to<std::vector>();
}

// Better: lazy evaluation for type checking
auto valid_types = registered_types
    | std::views::filter([](auto& t) { return validate_type(t).has_value(); });
```

**Benefits:**
- More declarative and readable type processing
- Lazy evaluation reduces memory usage
- Composable validation pipelines
- Better separation of filtering logic from processing

### 6. `std::mdspan` for Table Data Access

**Current approach:**
```cpp
// Custom iterators and accessors
for (const auto& row : table) { }
```

**C++23 approach:**
```cpp
template<typename T>
using TableView = std::mdspan<T, std::extents<size_t, std::dynamic_extent, field_count<T>()>>;

// Direct columnar access
auto names = table_view[std::full_extent, name_column];
```

**Benefits:**
- Standard library support for multi-dimensional access
- Optimized for columnar operations
- Compatible with parallel algorithms

## C++26 Transformative Features

> **Note**: The C++26 examples below are based on current proposals (particularly P2996 for reflection, which uses the `^` reification operator). The final C++26 standard may differ significantly as proposals evolve through the standardization process. These examples are illustrative of the *capabilities* that reflection would enable, not necessarily the exact syntax that will be standardized.

### 1. Static Reflection (P2996) - Complete Macro Elimination

**Current approach:**
```cpp
SPACETIMEDB_STRUCT(User, id, name, email)
SPACETIMEDB_TABLE(User, users, Public)
FIELD_PrimaryKey(users, id);
SPACETIMEDB_ENUM(UserRole, Admin, Moderator, Member)
SPACETIMEDB_NAMESPACE(UserRole, "Auth")  // Separate macro for namespace
```

**Illustrative C++26 approach** (exact syntax TBD in standardization):
```cpp
// Natural C++ attributes replace macros
struct [[spacetimedb::table("users", public)]] User {
    [[spacetimedb::primary_key]] uint32_t id;
    [[spacetimedb::unique]] std::string email;
    std::string name;
};

enum class [[spacetimedb::namespace("Auth")]] UserRole {
    Admin, Moderator, Member
};

// Automatic registration via reflection (pseudocode - actual API TBD)
template<typename T> requires has_spacetimedb_table_attr<T>
consteval void register_table() {
    for (constexpr auto member : reflect_members_of<T>()) {
        if constexpr (has_attribute<member, spacetimedb::primary_key>) {
            register_primary_key<T>(member.name(), member.type());
        }
    }
}

// Automatic namespace detection via reflection
template<typename T>
consteval std::string get_qualified_name() {
    if constexpr (has_attribute<T, spacetimedb::namespace>) {
        return get_namespace_attribute<T>() + "." + get_type_name<T>();
    }
    return get_type_name<T>();
}
```

**Benefits:**
- **Zero macros needed**
- Natural C++ syntax with attributes
- Complete type information at compile-time
- Automatic serialization without SPACETIMEDB_STRUCT

### 2. Contracts for Constraint Validation

**Current approach:**
```cpp
static_assert(FilterableValue<FieldType>, "Field cannot have Index constraint");
```

**C++26 approach:**
```cpp
template<typename T>
void add_index_constraint(T TableType::*field)
    [[pre: FilterableValue<T>]]
    [[pre: !has_existing_index(field)]]
{
    // Contract violations become compile-time or runtime errors
    // depending on evaluation context
}
```

**Benefits:**
- Declarative constraint specification
- Automatic error messages from contract violations
- Works for both compile-time and runtime validation

### 3. Pattern Matching for Type Dispatch

**Current approach:**
```cpp
switch(type.tag()) {
    case AlgebraicTypeTag::Product: ...
    case AlgebraicTypeTag::Sum: ...
    // Manual casting and handling
}
```

**C++26 approach:**
```cpp
inspect(type) {
    <Product> p => serialize_product(p),
    <Sum> s => serialize_sum(s),
    <Array> [auto elem_type] => serialize_array(elem_type),
    <Option> opt => serialize_option(opt),
    _ => throw InvalidType{}
}
```

**Benefits:**
- Type-safe pattern matching
- Exhaustiveness checking
- Cleaner type handling code

### 4. Compile-Time Type Registration via Reflection

**Current approach:**
```cpp
extern "C" __attribute__((export_name("__preinit__20_register_table_User")))
void __preinit__20_register_table_User() {
    Module::RegisterTable<User>("users", true);
}
```

**C++26 approach:**
```cpp
// Automatic discovery and registration at compile time
template<typename... Tables>
consteval auto generate_module_descriptor() {
    ModuleDescriptor desc;
    (reflect_and_register<Tables>(desc), ...);
    return desc;
}

// Single compile-time constant contains entire module
constexpr auto module = generate_module_descriptor<
    discover_tables_via_reflection()...  // Find all types with [[spacetimedb::table]]
>();

// Single runtime export
extern "C" const uint8_t* __describe_module__() {
    return module.serialize();  // Already computed at compile-time
}
```

**Benefits:**
- **No __preinit__ functions needed**
- Entire module structure known at compile-time
- Single binary blob computed during compilation
- Zero runtime registration overhead

### 5. Improved `constexpr` Allocation

**Current approach:**
```cpp
// Runtime type vector building
std::vector<AlgebraicType> types;
```

**C++26 approach:**
```cpp
constexpr auto build_typespace() {
    std::vector<AlgebraicType> types;
    // Fully constexpr vector operations
    for (auto type : reflect_all_types()) {
        types.push_back(analyze_type(type));
    }
    return types;
}

constexpr auto typespace = build_typespace();
```

**Benefits:**
- Complete typespace computed at compile-time
- No runtime allocation or registration
- Enables full compile-time validation

### 5b. First-Class Error Type Support

**Current approach** (workaround using Outcome<void>):
```cpp
SPACETIMEDB_REDUCER(process, ReducerContext ctx, uint32_t id) {
    if (id == 0) {
        return Err("Invalid ID");  // Error represented as string
    }
    ctx.db[users].insert({id});
    return Ok();  // No value, just success
}
```

**C++26 potential approach** (with better Result types):
```cpp
// Could use std::expected with richer error information
template<typename T, typename E = std::string>
using Result = std::expected<T, E>;

SPACETIMEDB_REDUCER(process, ReducerContext ctx, uint32_t id) {
    if (id == 0) {
        return std::unexpected(ProcessError::InvalidId);  // Type-safe errors
    }
    ctx.db[users].insert({id});
    return {};  // Clear success value
}
```

**Benefits:**
- Richer error types beyond string messages
- Better IDE support for error handling
- Cleaner syntax for success/error distinction
- Compile-time error case exhaustiveness checking

### 6. Reflection-Based Serialization

**Current approach:**
```cpp
// Manual field listing in SPACETIMEDB_STRUCT macro
SPACETIMEDB_STRUCT(MyType, field1, field2, field3)
```

**C++26 approach:**
```cpp
template<typename T>
constexpr void serialize(Writer& w, const T& obj) {
    [:expand(^T::members()):] >> [&]<auto member> {
        w.write(obj.[:member:]);
    };
}

// Automatic serialization for any struct, no macros needed
```

**Benefits:**
- Automatic serialization for all types
- No manual field enumeration
- Works with any struct/class

## Migration Strategy

### Phase 1: C++23 Adoption
1. Replace class hierarchies with deducing `this`
2. Introduce `std::expected` for error handling
3. Use `if consteval` for hybrid validation
4. Implement `constexpr` type tree building
5. Gradually reduce macro usage

### Phase 2: C++26 Revolution
1. Replace all macros with reflection
2. Eliminate __preinit__ system entirely
3. Move to compile-time module generation
4. Implement pattern matching for type dispatch
5. Use contracts for all constraint validation

## Expected Outcomes

### With C++23:
- **30-50% reduction** in macro usage
- **Faster compilation** through reduced template instantiation
- **Better error messages** with `std::expected`
- **Cleaner API** with deducing `this`

### With C++26:
- **100% macro elimination**
- **Zero runtime registration overhead**
- **Compile-time module validation**
- **Natural C++ syntax** without SDK-specific constructs
- **Full IDE support** (no macro magic hiding semantics)

## Performance Implications

> **Disclaimer**: The following performance projections are educated estimates based on similar language features and SDK patterns. Actual performance gains will depend on:
> - Compiler optimization capabilities
> - Final C++26 feature specifications
> - WASM code generation characteristics
> - Specific module structure (table count, field count, etc.)

### Compile-Time Performance
- **C++23**: Slightly increased due to more `constexpr` evaluation (estimated +5-15%)
- **C++26**: Significantly increased initially due to reflection-based analysis, but:
  - Module structure computed once during build and cached
  - No runtime registration code to compile
  - Potential for better code generation optimization
  - Long-term: compiler improvements may negate initial increases

### Runtime Performance
- **C++23**: ~5-15% improvement from reduced indirection and better inlining
- **C++26**: ~20-40% improvement from eliminating registration entirely
  - Actual gains depend on whether compiler can optimize registration code away
  - May be limited by WASM constraints

### WASM Binary Size
- **C++23**: ~5-10% reduction (less template instantiation)
- **C++26**: ~15-25% reduction (no registration code)
  - Note: Reflection metadata may add some size overhead if not stripped
  - Net reduction depends on compile-time constant optimization

## Conclusion

C++26's static reflection will enable a significant paradigm shift from runtime registration to compile-time module generation. The SpacetimeDB C++ bindings could achieve **zero-overhead abstractions** with no macros and no runtime registration - just pure, standard C++.

The journey through C++23 provides valuable incremental improvements:
- Cleaner APIs with `deducing this`
- Better error handling with `std::expected`  
- Hybrid validation with `if consteval`
- More efficient code generation

C++26's reflection capabilities will allow us to achieve compile-time type safety and module generation with natural C++ syntax, making the C++ bindings substantially more ergonomic and performant than currently possible.

**Important caveats:**
- C++26 reflection is still in proposal stage; final syntax and capabilities may differ
- Compiler support for these features will take time after standardization
- WASM toolchain support (Emscripten) will need updates
- Migration from C++20 code will require careful planning
- Not all performance gains may materialize depending on compiler optimizations and WASM constraints