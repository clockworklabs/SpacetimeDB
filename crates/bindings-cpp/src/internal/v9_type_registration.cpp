#include "spacetimedb/internal/v9_type_registration.h"
#include "spacetimedb/internal/v9_builder.h"
#include "spacetimedb/internal/autogen/AlgebraicType.g.h"
#include "spacetimedb/internal/autogen/RawModuleDefV9.g.h"
#include "spacetimedb/internal/autogen/RawTypeDefV9.g.h"
#include "spacetimedb/internal/autogen/ProductType.g.h"
#include "spacetimedb/internal/autogen/SumType.g.h"
#include "spacetimedb/logger.h"  // For LOG_ERROR, LOG_PANIC
#include <cxxabi.h>
#include <memory>
#include <algorithm>
#include <cstdio>
#include <stdexcept>

namespace SpacetimeDB {
namespace Internal {

// Use the correct namespaces - these types are already in the correct namespace

// Forward declaration of the global V9 module accessor
RawModuleDefV9& GetV9Module();

// Global V9 type registration instance
std::unique_ptr<V9TypeRegistration> g_v9_type_registration;

// Thread-local storage for tracking the chain of types being registered
thread_local std::vector<std::string> g_type_registration_chain;

// Global flag to indicate circular reference error (set during type building)
bool g_circular_ref_error = false;
std::string g_circular_ref_type_name;

void initializeV9TypeRegistration() {
    g_v9_type_registration = std::make_unique<V9TypeRegistration>();
    // Clear any previous error state
    g_circular_ref_error = false;
    g_circular_ref_type_name.clear();
    g_type_registration_chain.clear();
}

V9TypeRegistration& getV9TypeRegistration() {
    if (!g_v9_type_registration) {
        initializeV9TypeRegistration();
    }
    return *g_v9_type_registration;
}

// Function moved outside Internal namespace - see end of file

AlgebraicType V9TypeRegistration::registerType(const bsatn::AlgebraicType& bsatn_type,
                                               const std::string& explicit_name,
                                               const std::type_info* cpp_type) {
    
    // 1. PRIMITIVES - always inline, never registered
    if (isPrimitive(bsatn_type)) {
        return convertPrimitive(bsatn_type);
    }
    
    // 2. REFS - already registered, convert and return
    if (bsatn_type.tag() == bsatn::AlgebraicTypeTag::Ref) {
        AlgebraicType result(AlgebraicType::Tag::Ref);
        result.set<0>(bsatn_type.as_ref());
        return result;
    }
    
    // 3. ARRAYS - always inline with recursive element processing
    if (bsatn_type.tag() == bsatn::AlgebraicTypeTag::Array) {
        return convertArray(bsatn_type);
    }
    
    // 4. UNIT TYPES - empty products should be inlined as Unit ONLY if unnamed
    // Named unit structs (like UnitStruct) should be registered as named types
    if (isUnitType(bsatn_type)) {
        if (!explicit_name.empty()) {
            // This is a named unit struct like UnitStruct - do NOT inline
            // Fall through to register as named type
        } else {
            // Unnamed unit type - inline it
            return convertUnitType();
        }
    }
    
    // 5. SPECIAL TYPES - always inline
    if (isSpecialType(bsatn_type)) {
        return convertSpecialType(bsatn_type);
    }
    
    // Check if this might be ScheduleAt being registered with wrong name
    if (bsatn_type.tag() == bsatn::AlgebraicTypeTag::Sum) {
        const auto& sum = bsatn_type.as_sum();
        if (sum.variants.size() == 2 &&
            sum.variants[0].name == "Interval" &&
            sum.variants[1].name == "Time") {
            // This is ScheduleAt - should have been caught above!
            fprintf(stdout, "WARNING: ScheduleAt type detected but not inlined! explicit_name='%s'\n", 
                    explicit_name.c_str());
            // Force inline it
            return convertInlineSum(bsatn_type);
        }
    }
    
    // 6. OPTIONS - always inline
    if (isOptionType(bsatn_type)) {
        return convertInlineSum(bsatn_type);
    }
    
    // 7. RESULTS - always inline
    if (isResultType(bsatn_type)) {
        return convertInlineSum(bsatn_type);
    }
    
    // 8. SCHEDULE_AT - always inline
    if (isScheduleAtType(bsatn_type)) {
        return convertInlineSum(bsatn_type);
    }
    
    // ============================================================
    // ONLY USER-DEFINED STRUCTS AND ENUMS GET REGISTERED BELOW
    // ============================================================
    
    // 7. DETERMINE TYPE NAME
    std::string type_name = explicit_name;
    if (type_name.empty() && cpp_type) {
        type_name = extractTypeName(cpp_type);
    } else if (!type_name.empty()) {
        // Even if we have an explicit name, ensure it doesn't contain namespace separators
        // This fixes issues with special types like SpacetimeDB::ScheduleAt
        size_t last_colon = type_name.rfind("::");
        if (last_colon != std::string::npos) {
            type_name = type_name.substr(last_colon + 2);
        }
    }
    
    if (type_name.empty()) {
        // Complex types MUST have proper names - this is a validation error
        error_type_description_ = describeType(bsatn_type);
        error_message_ = "Missing type name for complex type: " + error_type_description_;
        has_error_ = true;
        
        // Return a dummy type to avoid crashing
        return AlgebraicType(AlgebraicType::Tag::U8);
    }
    
    // 8. DETECT RECURSIVE TYPE REGISTRATION (CYCLE DETECTION)
    if (types_being_registered_.find(type_name) != types_being_registered_.end()) {
        // Set error state for validation preinit to report
        error_message_ = "Recursive type reference detected: '" + type_name + "' is referencing itself";
        has_error_ = true;
        
        // Return a dummy type to break the infinite recursion
        // This allows the validation preinit to run and report the error cleanly
        return AlgebraicType(AlgebraicType::Tag::U8);
    }
    
    // 9. CHECK IF ALREADY REGISTERED
    // Check cache first
    auto cache_it = type_name_cache_.find(type_name);
    if (cache_it != type_name_cache_.end()) {
        AlgebraicType result(AlgebraicType::Tag::Ref);
        result.set<0>(cache_it->second);
        return result;
    }
    
    // Check module's types array
    for (const auto& type_def : GetV9Module().types) {
        if (type_def.name.name == type_name) {
            uint32_t typespace_index = type_def.ty;
            type_name_cache_[type_name] = typespace_index;
            AlgebraicType result(AlgebraicType::Tag::Ref);
            result.set<0>(typespace_index);
            return result;
        }
    }
    
    // 10. REGISTER NEW COMPLEX TYPE
    return registerComplexType(bsatn_type, type_name);
}

uint32_t V9TypeRegistration::registerAndGetIndex(const bsatn::AlgebraicType& bsatn_type,
                                                 const std::string& type_name,
                                                 const std::type_info* cpp_type) {
    // Check if already registered in cache
    auto cache_it = type_name_cache_.find(type_name);
    if (cache_it != type_name_cache_.end()) {
        return cache_it->second;
    }
    
    // Register the type and get the Internal::AlgebraicType result
    AlgebraicType result = registerType(bsatn_type, type_name, cpp_type);
    
    // If it's a Ref, return the index
    if (result.get_tag() == AlgebraicType::Tag::Ref) {
        uint32_t index = result.get<0>();
        type_name_cache_[type_name] = index;
        return index;
    }
    
    // This shouldn't happen for enums - they should always register as complex types
    fprintf(stderr, "ERROR: Enum '%s' did not register as a complex type\n", type_name.c_str());
    return 0;
}

void V9TypeRegistration::registerTypeByName(const std::string& type_name, 
                                            const bsatn::AlgebraicType& algebraic_type,
                                            [[maybe_unused]] const std::type_info* cpp_type) {
    // Check if already registered in cache
    auto cache_it = type_name_cache_.find(type_name);
    if (cache_it != type_name_cache_.end()) {
        fprintf(stdout, "ENUM DEBUG: Type '%s' already registered at index %u\n", 
                type_name.c_str(), cache_it->second);
        return;
    }
    
    // Check if already registered in module's types array
    for (const auto& type_def : GetV9Module().types) {
        if (type_def.name.name == type_name) {
            uint32_t typespace_index = type_def.ty;
            type_name_cache_[type_name] = typespace_index;
            fprintf(stdout, "ENUM DEBUG: Type '%s' found in module at index %u\n", 
                    type_name.c_str(), typespace_index);
            return;
        }
    }
    
    // Register new type immediately
    fprintf(stdout, "ENUM DEBUG: Registering new type '%s' immediately\n", type_name.c_str());
    
    // Mark this type as being registered (for cycle detection)
    types_being_registered_.insert(type_name);
    
    // Reserve space in typespace
    uint32_t typespace_index = GetV9Module().typespace.types.size();
    
    // For immediate registration, we process the type directly without recursive calls
    // This avoids infinite loops when the enum's algebraic_type() calls registerTypeByName
    AlgebraicType processed_type;
    
    if (algebraic_type.tag() == bsatn::AlgebraicTypeTag::Sum) {
        // For enums, directly process the Sum type structure without recursive registerType calls
        auto sum = std::make_unique<SumType>();
        
        for (const auto& variant : algebraic_type.as_sum().variants) {
            SumTypeVariant v;
            v.name = variant.name;
            
            // For simple enum variants, they should be Unit types (no payload)
            // Check if it's a Unit type and convert appropriately
            if (isUnitType(*variant.algebraic_type)) {
                v.algebraic_type = convertUnitType();
            } else if (isPrimitive(*variant.algebraic_type)) {
                // For primitive variant types
                v.algebraic_type = convertPrimitive(*variant.algebraic_type);
            } else {
                // For complex variant types, we need to recursively process them
                // But we can't call registerType here as it would cause infinite recursion
                // So we do a simple inline conversion
                if (variant.algebraic_type->tag() == bsatn::AlgebraicTypeTag::Array) {
                    v.algebraic_type = convertArray(*variant.algebraic_type);
                } else if (variant.algebraic_type->tag() == bsatn::AlgebraicTypeTag::Product) {
                    v.algebraic_type = convertSpecialType(*variant.algebraic_type);
                } else {
                    // Fallback to primitive conversion
                    v.algebraic_type = convertPrimitive(*variant.algebraic_type);
                }
            }
            sum->variants.push_back(std::move(v));
        }
        
        processed_type = AlgebraicType(AlgebraicType::Tag::Sum);
        processed_type.set<1>(std::move(sum));
    } else if (algebraic_type.tag() == bsatn::AlgebraicTypeTag::Product) {
        // For structs, process directly
        auto product = std::make_unique<ProductType>();
        
        for (const auto& field : algebraic_type.as_product().elements) {
            ProductTypeElement elem;
            elem.name = field.name;
            // Convert field type appropriately - handle different types
            if (isPrimitive(*field.algebraic_type)) {
                elem.algebraic_type = convertPrimitive(*field.algebraic_type);
            } else if (isUnitType(*field.algebraic_type)) {
                elem.algebraic_type = convertUnitType();
            } else {
                // For complex field types, just convert as primitive for now
                elem.algebraic_type = convertPrimitive(*field.algebraic_type);
            }
            product->elements.push_back(std::move(elem));
        }
        
        processed_type = AlgebraicType(AlgebraicType::Tag::Product);
        processed_type.set<2>(std::move(product));
    } else {
        fprintf(stderr, "[Warning] Unexpected type tag %d for '%s'\n", 
                static_cast<int>(algebraic_type.tag()), type_name.c_str());
        processed_type = convertPrimitive(algebraic_type);
    }
    
    // Add to typespace
    GetV9Module().typespace.types.push_back(processed_type);
    
    // Create RawTypeDefV9 export with namespace support
    RawTypeDefV9 type_def;
    
    // Parse namespace from type name
    auto [scope, simple_name] = parseNamespaceAndName(type_name);
    type_def.name.scope = scope;
    type_def.name.name = simple_name;
    type_def.ty = typespace_index;
    type_def.custom_ordering = true; // Complex types need custom ordering
    
    // Add to module's types array
    GetV9Module().types.push_back(type_def);
    
    // Update cache
    type_name_cache_[type_name] = typespace_index;
    
    // Remove from types being registered (cycle detection cleanup)
    types_being_registered_.erase(type_name);
    
    fprintf(stdout, "ENUM DEBUG: Successfully registered type '%s' at index %u\n", 
            type_name.c_str(), typespace_index);
}

bool V9TypeRegistration::isPrimitive(const bsatn::AlgebraicType& type) const {
    auto tag = static_cast<uint32_t>(type.tag());
    // Use range check: String (4) to F64 (19) covers all primitive types
    return tag >= static_cast<uint32_t>(bsatn::AlgebraicTypeTag::String) &&
           tag <= static_cast<uint32_t>(bsatn::AlgebraicTypeTag::F64);
}

bool V9TypeRegistration::isSpecialType(const bsatn::AlgebraicType& type) const {
    if (type.tag() != bsatn::AlgebraicTypeTag::Product) {
        return false;
    }
    
    const auto& product = type.as_product();
    if (product.elements.size() != 1) {
        return false;
    }
    
    const auto& field_name = product.elements[0].name;
    return field_name == "__identity__" ||
           field_name == "__connection_id__" ||
           field_name == "__timestamp_micros_since_unix_epoch__" ||
           field_name == "__time_duration_micros__" ||
           field_name == "__uuid__";
}

bool V9TypeRegistration::isOptionType(const bsatn::AlgebraicType& type) const {
    if (type.tag() != bsatn::AlgebraicTypeTag::Sum) {
        return false;
    }
    
    const auto& sum = type.as_sum();
    return sum.variants.size() == 2 &&
           sum.variants[0].name == "some" &&
           sum.variants[1].name == "none";
}

bool V9TypeRegistration::isResultType(const bsatn::AlgebraicType& type) const {
    if (type.tag() != bsatn::AlgebraicTypeTag::Sum) {
        return false;
    }
    
    const auto& sum = type.as_sum();
    return sum.variants.size() == 2 &&
           sum.variants[0].name == "ok" &&
           sum.variants[1].name == "err";
}

bool V9TypeRegistration::isScheduleAtType(const bsatn::AlgebraicType& type) const {
    if (type.tag() != bsatn::AlgebraicTypeTag::Sum) {
        return false;
    }
    
    const auto& sum = type.as_sum();
    return sum.variants.size() == 2 &&
           sum.variants[0].name == "Interval" &&
           sum.variants[1].name == "Time";
}

bool V9TypeRegistration::isUnitType(const bsatn::AlgebraicType& type) const {
    if (type.tag() != bsatn::AlgebraicTypeTag::Product) {
        return false;
    }
    
    const auto& product = type.as_product();
    // Unit type is an empty Product (no fields)
    return product.elements.empty();
}

AlgebraicType V9TypeRegistration::convertUnitType() const {
    // Create an empty Product type (Unit)
    auto product = std::make_unique<ProductType>();
    // No elements - empty Product
    
    AlgebraicType result(AlgebraicType::Tag::Product);
    result.set<2>(std::move(product));
    return result;
}

std::string V9TypeRegistration::extractTypeName(const std::type_info* cpp_type) const {
    std::string demangled = demangle_cpp_type_name(cpp_type->name());
    
    // Extract simple name (last component after ::)
    size_t last_colon = demangled.rfind("::");
    if (last_colon != std::string::npos) {
        demangled = demangled.substr(last_colon + 2);
    }
    
    // Remove template parameters
    size_t template_start = demangled.find('<');
    if (template_start != std::string::npos) {
        demangled = demangled.substr(0, template_start);
    }
    
    return demangled;
}

std::pair<std::vector<std::string>, std::string> V9TypeRegistration::parseNamespaceAndName(const std::string& qualified_name) const {
    std::vector<std::string> scope;
    std::string name;
    
    // Look for namespace separator (dot)
    size_t last_dot = qualified_name.rfind('.');
    if (last_dot != std::string::npos) {
        // Split into namespace and name
        std::string namespace_part = qualified_name.substr(0, last_dot);
        name = qualified_name.substr(last_dot + 1);
        
        // Split namespace into scope components (in case of nested namespaces like "A.B.C")
        size_t start = 0;
        size_t pos = 0;
        while ((pos = namespace_part.find('.', start)) != std::string::npos) {
            std::string component = namespace_part.substr(start, pos - start);
            if (!component.empty()) {
                scope.push_back(component);
            }
            start = pos + 1;
        }
        // Add the last component
        std::string last_component = namespace_part.substr(start);
        if (!last_component.empty()) {
            scope.push_back(last_component);
        }
        
        // fprintf(stdout, "DEBUG: Parsed namespace '%s' -> scope=[", qualified_name.c_str());
        // for (size_t i = 0; i < scope.size(); ++i) {
        //     if (i > 0) fprintf(stdout, ", ");
        //     fprintf(stdout, "\"%s\"", scope[i].c_str());
        // }
        // fprintf(stdout, "], name=\"%s\"\n", name.c_str());
    } else {
        // No namespace, just use the full name
        name = qualified_name;
        // scope remains empty
    }
    
    return std::make_pair(scope, name);
}

AlgebraicType V9TypeRegistration::convertPrimitive(const bsatn::AlgebraicType& type) const {
    switch (type.tag()) {
        case bsatn::AlgebraicTypeTag::Bool:
            return AlgebraicType(AlgebraicType::Tag::Bool);
        case bsatn::AlgebraicTypeTag::U8:
            return AlgebraicType(AlgebraicType::Tag::U8);
        case bsatn::AlgebraicTypeTag::U16:
            return AlgebraicType(AlgebraicType::Tag::U16);
        case bsatn::AlgebraicTypeTag::U32:
            return AlgebraicType(AlgebraicType::Tag::U32);
        case bsatn::AlgebraicTypeTag::U64:
            return AlgebraicType(AlgebraicType::Tag::U64);
        case bsatn::AlgebraicTypeTag::U128:
            return AlgebraicType(AlgebraicType::Tag::U128);
        case bsatn::AlgebraicTypeTag::U256:
            return AlgebraicType(AlgebraicType::Tag::U256);
        case bsatn::AlgebraicTypeTag::I8:
            return AlgebraicType(AlgebraicType::Tag::I8);
        case bsatn::AlgebraicTypeTag::I16:
            return AlgebraicType(AlgebraicType::Tag::I16);
        case bsatn::AlgebraicTypeTag::I32:
            return AlgebraicType(AlgebraicType::Tag::I32);
        case bsatn::AlgebraicTypeTag::I64:
            return AlgebraicType(AlgebraicType::Tag::I64);
        case bsatn::AlgebraicTypeTag::I128:
            return AlgebraicType(AlgebraicType::Tag::I128);
        case bsatn::AlgebraicTypeTag::I256:
            return AlgebraicType(AlgebraicType::Tag::I256);
        case bsatn::AlgebraicTypeTag::F32:
            return AlgebraicType(AlgebraicType::Tag::F32);
        case bsatn::AlgebraicTypeTag::F64:
            return AlgebraicType(AlgebraicType::Tag::F64);
        case bsatn::AlgebraicTypeTag::String:
            return AlgebraicType(AlgebraicType::Tag::String);
        default:
            // Unknown primitive - use U8 as fallback
            return AlgebraicType(AlgebraicType::Tag::U8);
    }
}

AlgebraicType V9TypeRegistration::convertArray(const bsatn::AlgebraicType& type) {
    const auto& array = type.as_array();
    
    // Recursively process element type
    AlgebraicType elem_type = registerType(*array.element_type);
    
    // Create inline array
    AlgebraicType result(AlgebraicType::Tag::Array);
    result.set<3>(std::make_unique<AlgebraicType>(std::move(elem_type)));
    return result;
}

AlgebraicType V9TypeRegistration::convertSpecialType(const bsatn::AlgebraicType& type) {
    // Special types are inlined as Product structures
    auto product = std::make_unique<ProductType>();
    
    for (const auto& field : type.as_product().elements) {
        ProductTypeElement elem;
        elem.name = field.name;
        
        // Recursively process field type (should be a primitive)
        elem.algebraic_type = registerType(*field.algebraic_type);
        product->elements.push_back(std::move(elem));
    }
    
    AlgebraicType result(AlgebraicType::Tag::Product);
    result.set<2>(std::move(product));
    return result;
}

AlgebraicType V9TypeRegistration::convertInlineSum(const bsatn::AlgebraicType& type) {
    // Options and ScheduleAt are inlined as Sum structures
    auto sum = std::make_unique<SumType>();
    
    for (const auto& variant : type.as_sum().variants) {
        SumTypeVariant v;
        v.name = variant.name;
        
        // Recursively process variant type
        v.algebraic_type = registerType(*variant.algebraic_type);
        sum->variants.push_back(std::move(v));
    }
    
    AlgebraicType result(AlgebraicType::Tag::Sum);
    result.set<1>(std::move(sum));
    return result;
}

AlgebraicType V9TypeRegistration::registerComplexType(const bsatn::AlgebraicType& type,
                                                      const std::string& type_name) {
    // Mark this type as being registered (for cycle detection)
    types_being_registered_.insert(type_name);
    
    // Reserve space in typespace
    uint32_t typespace_index = GetV9Module().typespace.types.size();
    
    // Debug logging (disabled in production)
    #ifdef DEBUG_TYPE_REGISTRATION
    fprintf(stdout, "[Type] Registering '%s' at index %u\n", type_name.c_str(), typespace_index);
    #endif
    
    // Process the type based on its kind
    AlgebraicType processed_type;
    
    if (type.tag() == bsatn::AlgebraicTypeTag::Product) {
        processed_type = processProduct(type);
    } else if (type.tag() == bsatn::AlgebraicTypeTag::Sum) {
        processed_type = processSum(type);
    } else {
        // This shouldn't happen in normal usage
        fprintf(stderr, "[Warning] Unexpected type tag %d for '%s'\n", 
                static_cast<int>(type.tag()), type_name.c_str());
        // Fallback
        processed_type = convertPrimitive(type);
    }
    
    // Add to typespace
    GetV9Module().typespace.types.push_back(processed_type);
    
    // Create RawTypeDefV9 export with namespace support
    RawTypeDefV9 type_def;
    
    // Parse namespace from type name
    auto [scope, simple_name] = parseNamespaceAndName(type_name);
    type_def.name.scope = scope;
    type_def.name.name = simple_name;
    type_def.ty = typespace_index;
    type_def.custom_ordering = true; // Complex types need custom ordering
    
    // Add to module's types array
    GetV9Module().types.push_back(type_def);
    
    // Update cache
    type_name_cache_[type_name] = typespace_index;
    
    // Remove from types being registered (cycle detection cleanup)
    types_being_registered_.erase(type_name);
    
    // Return Ref to the registered type
    AlgebraicType result(AlgebraicType::Tag::Ref);
    result.set<0>(typespace_index);
    return result;
}

AlgebraicType V9TypeRegistration::processProduct(const bsatn::AlgebraicType& type) {
    auto product = std::make_unique<ProductType>();
    
    for (const auto& field : type.as_product().elements) {
        ProductTypeElement elem;
        elem.name = field.name;
        
        // Recursively register/process field type
        // For nested complex types, we need to ensure they're registered with proper names
        // The registerType call will handle primitives, arrays, special types, and complex types
        elem.algebraic_type = registerType(*field.algebraic_type);
        product->elements.push_back(std::move(elem));
    }
    
    AlgebraicType result(AlgebraicType::Tag::Product);
    result.set<2>(std::move(product));
    return result;
}

AlgebraicType V9TypeRegistration::processSum(const bsatn::AlgebraicType& type) {
    auto sum = std::make_unique<SumType>();
    
    for (const auto& variant : type.as_sum().variants) {
        SumTypeVariant v;
        v.name = variant.name;
        
        // Recursively register/process variant type
        v.algebraic_type = registerType(*variant.algebraic_type);
        sum->variants.push_back(std::move(v));
    }
    
    AlgebraicType result(AlgebraicType::Tag::Sum);
    result.set<1>(std::move(sum));
    return result;
}

std::string V9TypeRegistration::describeType(const bsatn::AlgebraicType& type) const {
    switch (type.tag()) {
        case bsatn::AlgebraicTypeTag::Bool: return "Bool";
        case bsatn::AlgebraicTypeTag::U8: return "U8";
        case bsatn::AlgebraicTypeTag::U16: return "U16";
        case bsatn::AlgebraicTypeTag::U32: return "U32";
        case bsatn::AlgebraicTypeTag::U64: return "U64";
        case bsatn::AlgebraicTypeTag::U128: return "U128";
        case bsatn::AlgebraicTypeTag::U256: return "U256";
        case bsatn::AlgebraicTypeTag::I8: return "I8";
        case bsatn::AlgebraicTypeTag::I16: return "I16";
        case bsatn::AlgebraicTypeTag::I32: return "I32";
        case bsatn::AlgebraicTypeTag::I64: return "I64";
        case bsatn::AlgebraicTypeTag::I128: return "I128";
        case bsatn::AlgebraicTypeTag::I256: return "I256";
        case bsatn::AlgebraicTypeTag::F32: return "F32";
        case bsatn::AlgebraicTypeTag::F64: return "F64";
        case bsatn::AlgebraicTypeTag::String: return "String";
        
        case bsatn::AlgebraicTypeTag::Array: {
            const auto& array = type.as_array();
            return "Array<" + describeType(*array.element_type) + ">";
        }
        
        case bsatn::AlgebraicTypeTag::Product: {
            const auto& product = type.as_product();
            if (product.elements.empty()) {
                return "Product{}";
            }
            
            std::string desc = "Product{";
            bool first = true;
            for (const auto& elem : product.elements) {
                if (!first) desc += ", ";
                first = false;
                
                if (elem.name.has_value()) {
                    desc += elem.name.value() + ": ";
                }
                desc += describeType(*elem.algebraic_type);
            }
            desc += "}";
            return desc;
        }
        
        case bsatn::AlgebraicTypeTag::Sum: {
            const auto& sum = type.as_sum();
            if (sum.variants.empty()) {
                return "Sum{}";
            }
            
            // Check if it's an Option
            if (isOptionType(type)) {
                return "Option<" + describeType(*sum.variants[0].algebraic_type) + ">";
            }
            
            std::string desc = "Sum{";
            bool first = true;
            for (const auto& variant : sum.variants) {
                if (!first) desc += " | ";
                first = false;
                
                desc += variant.name + ": " + describeType(*variant.algebraic_type);
            }
            desc += "}";
            return desc;
        }
        
        case bsatn::AlgebraicTypeTag::Ref:
            return "Ref(" + std::to_string(type.as_ref()) + ")";
            
        default:
            return "Unknown(tag=" + std::to_string(static_cast<int>(type.tag())) + ")";
    }
}

void V9TypeRegistration::updateTypeNameInModule(uint32_t type_index, const std::string& new_name) {
    // Get access to the global V9 module
    RawModuleDefV9& v9_module = GetV9Module();
    
    // Check if the type index is valid
    if (type_index >= v9_module.types.size()) {
        fprintf(stderr, "ERROR: Invalid type index %u for namespace update (max: %zu)\n", 
                type_index, v9_module.types.size());
        return;
    }
    
    // Parse the new name to extract namespace and name parts
    auto [scope, name] = parseNamespaceAndName(new_name);
    
    // Update the type definition's scoped name
    v9_module.types[type_index].name.scope = scope;
    v9_module.types[type_index].name.name = name;
    
    fprintf(stdout, "DEBUG: Updated type index %u name to '%s' with %zu scope parts\n",
            type_index, name.c_str(), scope.size());
}

// processOptionInnerType function removed - no longer needed
// Options now use the same LazyTypeRegistrar pattern as other types

} // namespace Internal

} // namespace SpacetimeDB