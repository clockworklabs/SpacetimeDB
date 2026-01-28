#ifndef SPACETIMEDB_MODULE_IMPL_H
#define SPACETIMEDB_MODULE_IMPL_H

/**
 * SpacetimeDB C++ bindings - Module Implementation
 * 
 * Core module system implementation with optimized type handling, 
 * table registration, and reducer management.
 * 
 * Architecture:
 * - Type traits for compile-time type detection
 * - BSATN serialization utilities
 * - Unified table and reducer registration
 * - V9 builder integration for constraints
 */

#include "Module.h"
#include "field_registration.h"
#include "../table_with_constraints.h"
#include <spacetimedb/abi/FFI.h>
#include "bsatn_adapters.h"
#include <spacetimedb/bsatn/algebraic_type.h>
#include <spacetimedb/bsatn/traits.h>
#include <spacetimedb/bsatn/type_extensions.h>
#include "autogen/Lifecycle.g.h"
#include "autogen/RawConstraintDefV9.g.h"
#include "autogen/RawUniqueConstraintDataV9.g.h"
#include "autogen/RawIndexDefV9.g.h"
#include "autogen/RawIndexAlgorithm.g.h"
#include "v9_builder.h"
#include <cstring>
#include <cstdio>
#include <vector>
#include <string>
#include <optional>
#include <functional>
#include <unordered_map>
#include <algorithm>
#include <utility>

namespace SpacetimeDB {
namespace Internal {

// =============================================================================
// TYPE TRAITS
// =============================================================================

// Detect unit structs
template<typename T>
struct is_unit_struct_type {
private:
    template<typename U>
    static auto test(int) -> decltype(U::__is_unit_type__, std::bool_constant<U::__is_unit_type__>{});
    template<typename>
    static std::false_type test(...);
public:
    static constexpr bool value = decltype(test<T>(0))::value;
};

template<typename T>
inline constexpr bool is_unit_struct_v = is_unit_struct_type<T>::value;

// Detect vector types
template<typename T>
struct is_vector_type : std::false_type {};

template<typename T, typename Alloc>
struct is_vector_type<std::vector<T, Alloc>> : std::true_type {};

// Detect optional types
template<typename T>
struct is_optional : std::false_type {};

template<typename T>
struct is_optional<std::optional<T>> : std::true_type {};

// Check for big integer types
template<typename T>
constexpr bool is_big_integer_v = std::is_same_v<T, ::SpacetimeDB::u128> ||
                                   std::is_same_v<T, ::SpacetimeDB::i128> ||
                                   std::is_same_v<T, ::SpacetimeDB::u256> ||
                                   std::is_same_v<T, ::SpacetimeDB::i256>;

// Check if type needs registry registration
template<typename T>
constexpr bool needs_type_registration_v = (!std::is_arithmetic_v<T> && 
                                          !std::is_same_v<T, std::string> &&
                                          !is_big_integer_v<T>) ||
                                          requires { bsatn::bsatn_traits<T>::algebraic_type(); };

// Get BSATN tag for primitive types
template<typename T>
struct type_id {
    static constexpr uint8_t value = []() {
        if constexpr (std::is_same_v<T, bool>) return 4;
        else if constexpr (std::is_same_v<T, uint8_t>) return 5;
        else if constexpr (std::is_same_v<T, int8_t>) return 6;
        else if constexpr (std::is_same_v<T, uint16_t>) return 7;
        else if constexpr (std::is_same_v<T, int16_t>) return 8;
        else if constexpr (std::is_same_v<T, uint32_t>) return 9;
        else if constexpr (std::is_same_v<T, int32_t>) return 10;
        else if constexpr (std::is_same_v<T, uint64_t>) return 11;
        else if constexpr (std::is_same_v<T, int64_t>) return 12;
        else if constexpr (std::is_same_v<T, float>) return 17;
        else if constexpr (std::is_same_v<T, double>) return 18;
        else if constexpr (std::is_same_v<T, std::string>) return 19;
        else return 0; // Complex type
    }();
};

// Check for BSATN traits
template<typename T, typename = void>
struct has_bsatn_traits : std::false_type {};

template<typename T>
struct has_bsatn_traits<T, std::void_t<decltype(bsatn::bsatn_traits<T>::deserialize(std::declval<bsatn::Reader&>()))>> : std::true_type {};

template<typename T>
constexpr bool has_bsatn_traits_v = has_bsatn_traits<T>::value;

// Check if type can be written inline
template<typename T>
constexpr bool is_basic_inlineable_v = 
    std::is_arithmetic_v<T> || std::is_same_v<T, std::string> ||
    std::is_same_v<T, Identity> || std::is_same_v<T, ConnectionId> || 
    std::is_same_v<T, Timestamp> || std::is_same_v<T, TimeDuration> ||
    std::is_same_v<T, u128> || std::is_same_v<T, u256> ||
    std::is_same_v<T, i128> || std::is_same_v<T, i256>;

// =============================================================================
// BINARY I/O UTILITIES
// =============================================================================

inline void write_u32(std::vector<uint8_t>& buf, uint32_t val) {
    bsatn::Writer writer(buf);
    writer.write_u32_le(val);
}

inline void write_string(std::vector<uint8_t>& buf, const std::string& str) {
    bsatn::Writer writer(buf);
    writer.write_string(str);
}

inline uint8_t read_u8(uint32_t source) {
    BytesSource src{source};
    BytesSourceReader reader(src);
    return reader.read_u8();
}

inline uint32_t read_u32(uint32_t source) {
    BytesSource src{source};
    BytesSourceReader reader(src);
    return reader.read_u32_le();
}

// =============================================================================
// TABLE REGISTRATION
// =============================================================================

// Apply constraints to table with optimized field lookup
inline void apply_table_constraints(RawModuleDef::Table& table, 
                                   const std::vector<FieldConstraintInfo>& constraints) {
    if (constraints.empty() || table.fields.empty()) return;
    
    // Build field name lookup map
    std::unordered_map<std::string, uint16_t> field_indices;
    field_indices.reserve(table.fields.size());
    for (size_t i = 0; i < table.fields.size(); ++i) {
        field_indices.emplace(table.fields[i].name, static_cast<uint16_t>(i));
    }
    
    // Pre-allocate constraint vectors
    std::vector<uint16_t> unique_fields, indexed_fields, autoinc_fields;
    unique_fields.reserve(constraints.size());
    indexed_fields.reserve(constraints.size());
    autoinc_fields.reserve(constraints.size());
    
    // Process constraints in single pass
    for (const auto& constraint : constraints) {
        if (constraint.field_name == nullptr) continue;
        
        auto field_it = field_indices.find(constraint.field_name);
        if (field_it == field_indices.end()) continue;
        
        const uint16_t field_idx = field_it->second;
        const auto constraint_flags = constraint.constraints;
        
        if (constraint_flags == FieldConstraint::PrimaryKey || constraint_flags == FieldConstraint::PrimaryKeyAuto) {
            table.primary_key = field_idx;
            unique_fields.push_back(field_idx);
            indexed_fields.push_back(field_idx);
        }
        else if (constraint_flags == FieldConstraint::Unique || constraint_flags == FieldConstraint::Identity) {
            unique_fields.push_back(field_idx);
            indexed_fields.push_back(field_idx);
        }
        else if (has_constraint(constraint_flags, FieldConstraint::Indexed)) {
            indexed_fields.push_back(field_idx);
        }
        
        if (has_constraint(constraint_flags, FieldConstraint::AutoInc)) {
            autoinc_fields.push_back(field_idx);
        }
    }
    
    // Sort and deduplicate
    auto sort_and_dedupe = [](std::vector<uint16_t>& vec) {
        if (!vec.empty()) {
            std::sort(vec.begin(), vec.end());
            vec.erase(std::unique(vec.begin(), vec.end()), vec.end());
        }
    };
    
    sort_and_dedupe(unique_fields);
    sort_and_dedupe(indexed_fields);
    sort_and_dedupe(autoinc_fields);
    
    // Move into table
    table.unique_columns = std::move(unique_fields);
    table.indexed_columns = std::move(indexed_fields);
    table.autoinc_columns = std::move(autoinc_fields);
}

// Extract fields from type and populate table structure
template<typename T>
void add_fields_for_type(RawModuleDef::Table& table) {
    auto algebraic_type = bsatn::bsatn_traits<T>::algebraic_type();
    
    if (algebraic_type.tag() != bsatn::AlgebraicTypeTag::Product) {
        return;
    }
    
    const auto& product = algebraic_type.as_product();
    const size_t field_count = product.elements.size();
    
    table.fields.reserve(field_count);
    
    // Type-specific storage for field names
    static std::map<const std::type_info*, std::vector<std::string>> type_field_storage;
    auto& field_names = type_field_storage[&typeid(T)];
    field_names.clear();
    field_names.reserve(field_count);
    
    // Update global descriptors
    auto& global_descriptors = get_table_descriptors();
    auto& descriptor = global_descriptors[&typeid(T)];
    descriptor.fields.clear();
    descriptor.fields.reserve(field_count);
    
    // Process fields
    for (size_t i = 0; i < field_count; ++i) {
        const auto& element = product.elements[i];
        
        std::string field_name = element.name.has_value() ? 
            element.name.value() : 
            "field_" + std::to_string(i);
        field_names.push_back(std::move(field_name));
        
        FieldInfo field;
        field.name = field_names[i].c_str();
        field.offset = i * sizeof(void*);
        field.size = sizeof(void*);
        field.type_id = 0;
        field.serialize = [](std::vector<uint8_t>&, const void*) {};
        table.fields.push_back(field);
        
        FieldDescriptor global_field;
        global_field.name = field_names[i];
        global_field.offset = field.offset;
        global_field.size = field.size;
        global_field.write_type = [](std::vector<uint8_t>&) {};
        global_field.get_algebraic_type = []() { return bsatn::AlgebraicType::U32(); };
        global_field.serialize = [](std::vector<uint8_t>&, const void*) {};
        global_field.get_type_name = []() { return std::string(); };
        descriptor.fields.push_back(std::move(global_field));
    }
}

// Unified table registration - single implementation
template<typename T>
void Module::RegisterTableInternalImpl(const char* name, bool is_public, 
                                      const std::vector<FieldConstraintInfo>& constraints) {
    RawModuleDef::Table table;
    table.name = name;
    table.is_public = is_public;
    table.type = &typeid(T);
    
    add_fields_for_type<T>(table);
    
    if (!constraints.empty()) {
        apply_table_constraints(table, constraints);
    }
    
    // V9 registration - always register tables with V9Builder
    getV9Builder().RegisterTable<T>(name, is_public);
    
    table.serialize = [](std::vector<uint8_t>& buf, const void* obj) {
        auto& module_def = GetModuleDef();
        auto it = module_def.table_indices.find(&typeid(T));
        if (it == module_def.table_indices.end()) return;
        
        const auto& table = module_def.tables[it->second];
        for (const auto& field : table.fields) {
            field.serialize(buf, obj);
        }
    };
    
    GetModuleDef().AddTable(std::move(table));
}

// Overload for tables without constraints
template<typename T>
void Module::RegisterTableInternalImpl(const char* name, bool is_public) {
    RegisterTableInternalImpl<T>(name, is_public, {});
}

// =============================================================================
// ARGUMENT DESERIALIZATION
// =============================================================================

template<typename T>
T read_arg(uint32_t source) {
    BytesSource src{source};
    
    if constexpr (is_unit_struct_v<T>) {
        return T{};
    } else if constexpr (!needs_type_registration_v<T>) {
        BytesSourceReader reader(src);
        if constexpr (std::is_same_v<T, bool>) {
            return reader.read_bool();
        } else if constexpr (std::is_same_v<T, uint8_t>) {
            return reader.read_u8();
        } else if constexpr (std::is_same_v<T, uint16_t>) {
            return reader.read_u16_le();
        } else if constexpr (std::is_same_v<T, uint32_t>) {
            return reader.read_u32_le();
        } else if constexpr (std::is_same_v<T, uint64_t>) {
            return reader.read_u64_le();
        } else if constexpr (std::is_same_v<T, int8_t>) {
            return reader.read_i8();
        } else if constexpr (std::is_same_v<T, int16_t>) {
            return reader.read_i16_le();
        } else if constexpr (std::is_same_v<T, int32_t>) {
            return reader.read_i32_le();
        } else if constexpr (std::is_same_v<T, int64_t>) {
            return reader.read_i64_le();
        } else if constexpr (std::is_same_v<T, float>) {
            return reader.read_f32_le();
        } else if constexpr (std::is_same_v<T, double>) {
            return reader.read_f64_le();
        } else if constexpr (std::is_same_v<T, std::string>) {
            return reader.read_string();
        } else {
            static_assert(!sizeof(T), "Unsupported primitive type");
        }
    } else if constexpr (has_bsatn_traits_v<T>) {
        std::vector<uint8_t> buffer;
        constexpr size_t CHUNK_SIZE = 256;
        uint8_t chunk[CHUNK_SIZE];
        
        while (true) {
            size_t requested = CHUNK_SIZE;
            size_t actual = requested;
            FFI::bytes_source_read(src, chunk, &actual);
            
            if (actual == 0) break;
            buffer.insert(buffer.end(), chunk, chunk + actual);
            if (actual < requested) break;
        }
        
        bsatn::Reader reader(buffer);
        return bsatn::bsatn_traits<T>::deserialize(reader);
    } else {
        static_assert(std::is_default_constructible_v<T>, 
                      "Type must be default constructible or have BSATN traits");
        return T{};
    }
}

// Read multiple arguments as tuple
template<typename... Args>
auto read_args_tuple(uint32_t args_source) {
    BytesSource src{args_source};
    
    // Handle single unit struct specially
    if constexpr (sizeof...(Args) == 1) {
        using FirstArg = std::tuple_element_t<0, std::tuple<Args...>>;
        if constexpr (is_unit_struct_v<FirstArg>) {
            return std::make_tuple(FirstArg{});
        }
    }
    
    if constexpr ((has_bsatn_traits_v<Args> || ...)) {
        std::vector<uint8_t> buffer;
        constexpr size_t CHUNK_SIZE = 256;
        uint8_t chunk[CHUNK_SIZE];
        
        while (true) {
            size_t requested = CHUNK_SIZE;
            size_t actual = requested;
            FFI::bytes_source_read(src, chunk, &actual);
            
            if (actual == 0) break;
            buffer.insert(buffer.end(), chunk, chunk + actual);
            if (actual < requested) break;
        }
        
        bsatn::Reader reader(buffer);
        return std::make_tuple(bsatn::bsatn_traits<Args>::deserialize(reader)...);
    } else {
        BytesSourceReader reader(src);
        return std::make_tuple([&reader]() -> Args {
            if constexpr (std::is_same_v<Args, uint32_t>) {
                return reader.read_u32_le();
            } else if constexpr (std::is_same_v<Args, int32_t>) {
                return reader.read_i32_le();
            } else if constexpr (std::is_same_v<Args, uint64_t>) {
                return reader.read_u64_le();
            } else if constexpr (std::is_same_v<Args, int64_t>) {
                return reader.read_i64_le();
            } else if constexpr (std::is_same_v<Args, bool>) {
                return reader.read_bool();
            } else if constexpr (std::is_same_v<Args, std::string>) {
                return reader.read_string();
            } else {
                return bsatn::bsatn_traits<Args>::deserialize(reader);
            }
        }()...);
    }
}

// =============================================================================
// REDUCER WRAPPERS
// =============================================================================

// Lifecycle reducer wrapper
template<typename Func>
void builtin_reducer_wrapper(Func func, ReducerContext& ctx, 
                           uint64_t sender_0, uint64_t sender_1, 
                           uint64_t sender_2, uint64_t sender_3) {
    std::array<uint8_t, 32> senderBytes{};
    memcpy(senderBytes.data(), &sender_0, sizeof(uint64_t));
    memcpy(senderBytes.data() + 8, &sender_1, sizeof(uint64_t));
    memcpy(senderBytes.data() + 16, &sender_2, sizeof(uint64_t));
    memcpy(senderBytes.data() + 24, &sender_3, sizeof(uint64_t));
    Identity sender(senderBytes);
    
    if constexpr (std::is_invocable_v<Func, ReducerContext>) {
        func(ctx);
    } else if constexpr (std::is_invocable_v<Func, ReducerContext, Identity>) {
        func(ctx, sender);
    }
}

// Generic reducer wrapper
template<typename... Args>
void spacetimedb_reducer_wrapper(void (*func)(ReducerContext, Args...), 
                                ReducerContext& ctx, uint32_t args_source) {
    if constexpr (sizeof...(Args) == 0) {
        func(ctx);
    } else {
        auto args_tuple = read_args_tuple<Args...>(args_source);
        std::apply([&](auto&&... args) {
            func(ctx, std::forward<decltype(args)>(args)...);
        }, args_tuple);
    }
}

// =============================================================================
// TYPE SERIALIZATION
// =============================================================================

// Write algebraic type inline (for special types and primitives)
template<typename T>
void write_algebraic_type_inline(std::vector<uint8_t>& buf) {
    if constexpr (is_optional<T>::value) {
        using inner_type = typename T::value_type;
        
        buf.push_back(1); // AlgebraicType::Sum
        write_u32(buf, 2); // 2 variants
        
        buf.push_back(0); // Has name
        write_string(buf, "some");
        write_algebraic_type_inline<inner_type>(buf);
        
        buf.push_back(0); // Has name
        write_string(buf, "none");
        buf.push_back(2); // AlgebraicType::Product (empty)
        write_u32(buf, 0); // 0 elements
        
    } else if constexpr (is_vector_type<T>::value) {
        using element_type = typename T::value_type;
        buf.push_back(3); // AlgebraicType::Array
        write_algebraic_type_inline<element_type>(buf);
        
    } else if constexpr (std::is_same_v<T, int32_t>) {
        buf.push_back(10);
    } else if constexpr (std::is_same_v<T, uint32_t>) {
        buf.push_back(11);
    } else if constexpr (std::is_same_v<T, std::string>) {
        buf.push_back(4);
    } else if constexpr (std::is_same_v<T, bool>) {
        buf.push_back(5);
    } else if constexpr (std::is_same_v<T, int8_t>) {
        buf.push_back(6);
    } else if constexpr (std::is_same_v<T, uint8_t>) {
        buf.push_back(7);
    } else if constexpr (std::is_same_v<T, int16_t>) {
        buf.push_back(8);
    } else if constexpr (std::is_same_v<T, uint16_t>) {
        buf.push_back(9);
    } else if constexpr (std::is_same_v<T, int64_t>) {
        buf.push_back(12);
    } else if constexpr (std::is_same_v<T, uint64_t>) {
        buf.push_back(13);
    } else if constexpr (std::is_same_v<T, float>) {
        buf.push_back(18);
    } else if constexpr (std::is_same_v<T, double>) {
        buf.push_back(19);
    } else if constexpr (std::is_same_v<T, u128>) {
        buf.push_back(15);
    } else if constexpr (std::is_same_v<T, u256>) {
        buf.push_back(17);
    } else if constexpr (std::is_same_v<T, i128>) {
        buf.push_back(14);
    } else if constexpr (std::is_same_v<T, i256>) {
        buf.push_back(16);
    } else if constexpr (std::is_same_v<T, Identity>) {
        buf.push_back(2); // AlgebraicType::Product
        write_u32(buf, 1); // 1 field
        buf.push_back(0); // Some (has name)
        write_string(buf, bsatn::IDENTITY_TAG);
        buf.push_back(17); // AlgebraicType::U256
    } else if constexpr (std::is_same_v<T, ConnectionId>) {
        buf.push_back(2); // AlgebraicType::Product
        write_u32(buf, 1); // 1 field
        buf.push_back(0); // Some (has name)
        write_string(buf, bsatn::CONNECTION_ID_TAG);
        buf.push_back(15); // AlgebraicType::U128
    } else if constexpr (std::is_same_v<T, Timestamp>) {
        buf.push_back(2); // AlgebraicType::Product
        write_u32(buf, 1); // 1 field
        buf.push_back(0); // Some (has name)
        write_string(buf, bsatn::TIMESTAMP_TAG);
        buf.push_back(12); // AlgebraicType::I64
    } else if constexpr (std::is_same_v<T, TimeDuration>) {
        buf.push_back(2); // AlgebraicType::Product
        write_u32(buf, 1); // 1 field
        buf.push_back(0); // Some (has name)
        write_string(buf, bsatn::TIME_DURATION_TAG);
        buf.push_back(12); // AlgebraicType::I64
    } else if constexpr (std::is_enum_v<T>) {
        buf.push_back(4); // String fallback for complex enums
    } else {
        buf.push_back(4); // AlgebraicType::String fallback
    }
}

// =============================================================================
// REDUCER REGISTRATION
// =============================================================================

inline std::optional<Lifecycle> get_lifecycle_for_name(const std::string& name) {
    if (name == "init") return Lifecycle::Init;
    if (name == "client_connected") return Lifecycle::OnConnect;
    if (name == "client_disconnected") return Lifecycle::OnDisconnect;
    return std::nullopt;
}

// Unified reducer registration
template<typename... Args>
void RegisterReducerUnified(const std::string& name, 
                           void (*func)(ReducerContext, Args...), 
                           std::optional<Lifecycle> lifecycle = std::nullopt,
                           const std::vector<std::string>& param_names = {}) {
    RawModuleDef::Reducer reducer;
    reducer.name = name;
    reducer.lifecycle = lifecycle;
    
    reducer.handler = [func](ReducerContext& ctx, uint32_t args) {
        spacetimedb_reducer_wrapper(func, ctx, args);
    };
    
    if constexpr (sizeof...(Args) == 0) {
        reducer.write_params = [](std::vector<uint8_t>& buf) {
            write_u32(buf, 0);
        };
        reducer.param_names = {};
    } else {
        reducer.param_names = param_names;
    }
    
    // V9 registration
    {
        auto& v9_builder = getV9Builder();
        
        std::vector<bsatn::AlgebraicType> param_types;
        std::vector<const std::type_info*> param_cpp_types;
        std::vector<std::string> param_type_names;
        
        if constexpr (sizeof...(Args) > 0) {
            (param_types.push_back(bsatn::bsatn_traits<Args>::algebraic_type()), ...);
            (param_cpp_types.push_back(&typeid(Args)), ...);
            param_type_names.resize(sizeof...(Args));
        }
        
        v9_builder.AddV9Reducer(
            name,
            param_types,
            param_names,
            param_cpp_types,
            param_type_names,
            lifecycle
        );
    }
    
    Module::GetModuleDef().AddReducer(std::move(reducer));
}

// Lifecycle reducer registration
inline void RegisterLifecycleReducer(const std::string& name, 
                             std::optional<Lifecycle> lifecycle,
                             std::function<void(ReducerContext&, uint32_t)> handler) {
    auto& v9_builder = getV9Builder();
    
    v9_builder.AddV9Reducer(
        name,
        {},
        {},
        {},
        {},
        lifecycle
    );
    
    RawModuleDef::Reducer reducer;
    reducer.name = name;
    reducer.lifecycle = lifecycle;
    reducer.handler = handler;
    reducer.write_params = [](std::vector<uint8_t>& buf) {
        write_u32(buf, 0);
    };
    
    Module::GetModuleDef().AddReducer(std::move(reducer));
}

template<typename... Args>
void Module::RegisterReducerInternalImpl(const std::string& name, void (*func)(ReducerContext, Args...)) {
    RegisterReducerUnified(name, func, get_lifecycle_for_name(name));
}

template<typename... Args>
void Module::RegisterReducerInternalWithNames(const std::string& name, void (*func)(ReducerContext, Args...), const std::vector<std::string>& param_names) {
    RegisterReducerUnified(name, func, get_lifecycle_for_name(name), param_names);
}

inline void Module::RegisterInitReducer(void (*func)(ReducerContext)) {
    RegisterLifecycleReducer("init", Lifecycle::Init, 
        [func](ReducerContext& ctx, uint32_t) { func(ctx); });
}

inline void Module::RegisterClientConnectedReducer(void (*func)(ReducerContext, Identity)) {
    RegisterLifecycleReducer("client_connected", Lifecycle::OnConnect,
        [func](ReducerContext& ctx, uint32_t) { func(ctx, ctx.sender); });
}

inline void Module::RegisterClientDisconnectedReducer(void (*func)(ReducerContext, Identity)) {
    RegisterLifecycleReducer("client_disconnected", Lifecycle::OnDisconnect,
        [func](ReducerContext& ctx, uint32_t) { func(ctx, ctx.sender); });
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

template<typename T>
void register_table_type(const char* name, bool is_public) {
    Module::RegisterTableInternalImpl<T>(name, is_public);
}

// Optimized parameter name parsing
inline std::vector<std::string> parse_parameter_names(const std::string& params_str) {
    std::vector<std::string> names;
    
    size_t pos = 0;
    bool first_param = true;
    
    while (pos < params_str.length()) {
        size_t comma_pos = params_str.find(',', pos);
        if (comma_pos == std::string::npos) comma_pos = params_str.length();
        
        std::string param = params_str.substr(pos, comma_pos - pos);
        
        // Skip ReducerContext
        if (first_param) {
            first_param = false;
            pos = comma_pos + 1;
            continue;
        }
        
        // Trim whitespace
        size_t start = param.find_first_not_of(" \t");
        if (start != std::string::npos) {
            param = param.substr(start);
            size_t end = param.find_last_not_of(" \t");
            if (end != std::string::npos) {
                param = param.substr(0, end + 1);
            }
        }
        
        // Extract parameter name
        size_t last_space = param.find_last_of(" \t");
        if (last_space != std::string::npos) {
            std::string param_name = param.substr(last_space + 1);
            size_t name_end = param_name.find_first_of("&*[]");
            if (name_end != std::string::npos) {
                param_name = param_name.substr(0, name_end);
            }
            names.push_back(std::move(param_name));
        }
        
        pos = comma_pos + 1;
    }
    
    return names;
}

} // namespace Internal
} // namespace SpacetimeDB

#endif // SPACETIMEDB_MODULE_IMPL_H