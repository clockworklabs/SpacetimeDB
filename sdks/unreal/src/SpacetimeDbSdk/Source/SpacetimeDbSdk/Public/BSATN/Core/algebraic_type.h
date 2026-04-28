#ifndef SPACETIMEDB_BSATN_ALGEBRAIC_TYPE_H
#define SPACETIMEDB_BSATN_ALGEBRAIC_TYPE_H

#include <cstdint>
#include <string>
#include <vector>
#include <memory>
#include <variant>
#include <optional>
#include <stdexcept>
#include <type_traits>

namespace SpacetimeDb::bsatn {

// Forward declarations
class AlgebraicType;
struct SumTypeSchema;
struct ProductType;
struct ProductTypeElement;
struct SumTypeVariant;
struct ArrayType;

/**
 * Represents the tag for different algebraic types in SpacetimeDB's type system.
 * This mirrors the Rust/C# implementation for compatibility.
 */
enum class AlgebraicTypeTag : uint8_t {
    Ref = 0,      // Reference to another type
    Sum = 1,      // Sum type (tagged union/enum)
    Product = 2,  // Product type (struct/tuple)
    Array = 3,    // Array type
    String = 4,   // UTF-8 string
    Bool = 5,     // Boolean
    I8 = 6,       // Signed 8-bit integer
    U8 = 7,       // Unsigned 8-bit integer
    I16 = 8,      // Signed 16-bit integer
    U16 = 9,      // Unsigned 16-bit integer
    I32 = 10,     // Signed 32-bit integer
    U32 = 11,     // Unsigned 32-bit integer
    I64 = 12,     // Signed 64-bit integer
    U64 = 13,     // Unsigned 64-bit integer
    I128 = 14,    // Signed 128-bit integer
    U128 = 15,    // Unsigned 128-bit integer
    I256 = 16,    // Signed 256-bit integer
    U256 = 17,    // Unsigned 256-bit integer
    F32 = 18,     // 32-bit floating point
    F64 = 19      // 64-bit floating point
};

// ============================================================================
// HELPER TEMPLATE FOR DEEP COPYING UNIQUE_PTR MEMBERS
// ============================================================================

/**
 * RAII helper for deep copying unique_ptr members.
 * Eliminates duplicate copy constructor patterns.
 */
template<typename T>
std::unique_ptr<T> deep_copy_ptr(const std::unique_ptr<T>& ptr) {
    return ptr ? ptr->copy() : nullptr;
}

// ============================================================================
// TYPE ELEMENT STRUCTURES
// ============================================================================

/**
 * Represents an element in a ProductType.
 * Stores a complete AlgebraicType object to eliminate ambiguity.
 */
struct ProductTypeElement {
    std::optional<std::string> name;
    std::unique_ptr<AlgebraicType> algebraic_type;
    
    ProductTypeElement(std::optional<std::string> n, AlgebraicType type);
    ProductTypeElement(const ProductTypeElement& other);
    ProductTypeElement(ProductTypeElement&& other) = default;
    ProductTypeElement& operator=(const ProductTypeElement& other);
    ProductTypeElement& operator=(ProductTypeElement&& other) = default;
};

/**
 * Represents a variant in a SumType.
 * Stores a complete AlgebraicType object to eliminate ambiguity.
 */
struct SumTypeVariant {
    std::string name;
    std::unique_ptr<AlgebraicType> algebraic_type;
    
    SumTypeVariant(std::string n, AlgebraicType type);
    SumTypeVariant(const SumTypeVariant& other);
    SumTypeVariant(SumTypeVariant&& other) = default;
    SumTypeVariant& operator=(const SumTypeVariant& other);
    SumTypeVariant& operator=(SumTypeVariant&& other) = default;
};

/**
 * Represents a sum type (tagged union/enum).
 * Each variant has a name and can contain data.
 */
struct SumTypeSchema {
    std::vector<SumTypeVariant> variants;
    
    explicit SumTypeSchema(std::vector<SumTypeVariant> v) : variants(std::move(v)) {}
};

/**
 * Represents a product type (struct/tuple).
 * Contains ordered elements (fields).
 */
struct ProductType {
    std::vector<ProductTypeElement> elements;
    
    explicit ProductType(std::vector<ProductTypeElement> elems) 
        : elements(std::move(elems)) {}
    
    // Helper to create a product type for a C++ struct
    template<typename T>
    static ProductType make();
};

/**
 * Represents an array type.
 * Contains the complete type of elements in the array.
 */
struct ArrayType {
    std::unique_ptr<AlgebraicType> element_type;
    
    explicit ArrayType(AlgebraicType elem_type);
    ArrayType(const ArrayType& other);
    ArrayType(ArrayType&& other) = default;
    ArrayType& operator=(const ArrayType& other);
    ArrayType& operator=(ArrayType&& other) = default;
};

// ============================================================================
// MAIN ALGEBRAIC TYPE CLASS
// ============================================================================

/**
 * @brief The main algebraic type representation for SpacetimeDB's type system.
 * 
 * AlgebraicType is a tagged union that represents all possible types in SpacetimeDB.
 * It supports both primitive types (integers, floats, strings, etc.) and composite
 * types (products/structs, sums/enums, arrays, and references).
 * 
 * This type system is designed to be:
 * - Compatible with multiple languages (Rust, C#, C++)
 * - Serializable via BSATN (Binary Sparse Algebraic Type Notation)
 * - Type-safe with compile-time verification
 * 
 * @example Creating primitive types:
 * @code
 * auto int_type = AlgebraicType::primitive<AlgebraicTypeTag::I32>();
 * auto string_type = AlgebraicType::primitive<AlgebraicTypeTag::String>();
 * @endcode
 * 
 * @example Creating composite types:
 * @code
 * // Create an array type
 * auto array_type = AlgebraicType::Array(AlgebraicType::primitive<AlgebraicTypeTag::I32>());
 * @endcode
 */
class AlgebraicType {
public:
    /**
     * Internal data storage for type-specific information.
     */
    using DataType = std::variant<
        uint32_t,                          // Ref - type reference
        std::unique_ptr<SumTypeSchema>,    // Sum type
        std::unique_ptr<ProductType>,      // Product type
        std::unique_ptr<ArrayType>,        // Array type
        std::monostate                     // Primitive types (no additional data)
    >;

private:
    AlgebraicTypeTag tag_;
    DataType data_;

public:
    // Constructor accessible for copy operations and internal use
    AlgebraicType(AlgebraicTypeTag tag, DataType data) : tag_(tag), data_(std::move(data)) {}
    // -------------------------------------------------------------------------
    // PRIMITIVE TYPE FACTORY (TEMPLATED - REPLACES 16 INDIVIDUAL METHODS)
    // -------------------------------------------------------------------------
    
    /**
     * Template factory for primitive types.
     * Replaces all individual primitive factory methods (Bool(), I8(), etc.)
     * 
     * @tparam Tag The AlgebraicTypeTag for the primitive type
     * @return AlgebraicType instance for the specified primitive
     * 
     * @example
     * auto bool_type = AlgebraicType::primitive<AlgebraicTypeTag::Bool>();
     * auto int_type = AlgebraicType::primitive<AlgebraicTypeTag::I32>();
     */
    template<AlgebraicTypeTag Tag>
    static AlgebraicType primitive() {
        static_assert(
            static_cast<uint8_t>(Tag) >= static_cast<uint8_t>(AlgebraicTypeTag::String),
            "primitive<Tag>() can only be used for primitive types"
        );
        return AlgebraicType(Tag, std::monostate{});
    }
    
    // Convenience aliases for ALL primitives (required by other files)
    static AlgebraicType Bool() { return primitive<AlgebraicTypeTag::Bool>(); }
    static AlgebraicType I8() { return primitive<AlgebraicTypeTag::I8>(); }
    static AlgebraicType U8() { return primitive<AlgebraicTypeTag::U8>(); }
    static AlgebraicType I16() { return primitive<AlgebraicTypeTag::I16>(); }
    static AlgebraicType U16() { return primitive<AlgebraicTypeTag::U16>(); }
    static AlgebraicType I32() { return primitive<AlgebraicTypeTag::I32>(); }
    static AlgebraicType U32() { return primitive<AlgebraicTypeTag::U32>(); }
    static AlgebraicType I64() { return primitive<AlgebraicTypeTag::I64>(); }
    static AlgebraicType U64() { return primitive<AlgebraicTypeTag::U64>(); }
    static AlgebraicType I128() { return primitive<AlgebraicTypeTag::I128>(); }
    static AlgebraicType U128() { return primitive<AlgebraicTypeTag::U128>(); }
    static AlgebraicType I256() { return primitive<AlgebraicTypeTag::I256>(); }
    static AlgebraicType U256() { return primitive<AlgebraicTypeTag::U256>(); }
    static AlgebraicType F32() { return primitive<AlgebraicTypeTag::F32>(); }
    static AlgebraicType F64() { return primitive<AlgebraicTypeTag::F64>(); }
    static AlgebraicType String() { return primitive<AlgebraicTypeTag::String>(); }
    
    // -------------------------------------------------------------------------
    // COMPOSITE TYPE FACTORIES
    // -------------------------------------------------------------------------
    
    static AlgebraicType Ref(uint32_t type_id) {
        return AlgebraicType(AlgebraicTypeTag::Ref, type_id);
    }
    
    static AlgebraicType make_ref(uint32_t type_id) {
        return Ref(type_id);
    }
    
    static AlgebraicType make_product(std::unique_ptr<ProductType> product_type) {
        return AlgebraicType(AlgebraicTypeTag::Product, std::move(product_type));
    }
    
    static AlgebraicType make_sum(std::unique_ptr<SumTypeSchema> sum_type) {
        return AlgebraicType(AlgebraicTypeTag::Sum, std::move(sum_type));
    }
    
    static AlgebraicType Array(AlgebraicType elem_type) {
        return AlgebraicType(AlgebraicTypeTag::Array, 
                           std::make_unique<ArrayType>(std::move(elem_type)));
    }
    
    /**
     * Creates a unit type (empty product).
     * This represents std::monostate or Rust's () unit type.
     */
    static AlgebraicType Unit() {
        return make_product(std::make_unique<ProductType>(std::vector<ProductTypeElement>{}));
    }
    
    /**
     * Creates an Option type with simplified logic.
     * Represents a sum type with "some" and "none" variants.
     */
    static AlgebraicType Option(uint32_t some_type_ref) {
        if (some_type_ref == 0xFFFFFFFF) {
            return create_unit_option();
        }
        return create_typed_option(some_type_ref);
    }
    
    static AlgebraicType Product(std::vector<std::pair<std::string, uint32_t>> fields) {
        std::vector<ProductTypeElement> elements;
        elements.reserve(fields.size());
        for (auto& [name, type_ref] : fields) {
            elements.emplace_back(std::move(name), Ref(type_ref));
        }
        return make_product(std::make_unique<ProductType>(std::move(elements)));
    }
    
    // -------------------------------------------------------------------------
    // ACCESSORS AND TYPE CHECKING
    // -------------------------------------------------------------------------
    
    AlgebraicTypeTag tag() const { return tag_; }
    const DataType& data() const { return data_; }
    
    /**
     * Template-based type checking.
     * Replaces all individual is_*() methods with a single template.
     * 
     * @tparam Tag The AlgebraicTypeTag to check against
     * @return true if this type has the specified tag
     * 
     * @example
     * if (type.is<AlgebraicTypeTag::I32>()) { ... }
     * if (type.is<AlgebraicTypeTag::Array>()) { ... }
     */
    template<AlgebraicTypeTag Tag>
    bool is() const { return tag_ == Tag; }
    
    // Convenience methods for most common checks
    bool is_ref() const { return is<AlgebraicTypeTag::Ref>(); }
    bool is_sum() const { return is<AlgebraicTypeTag::Sum>(); }
    bool is_product() const { return is<AlgebraicTypeTag::Product>(); }
    bool is_array() const { return is<AlgebraicTypeTag::Array>(); }
    bool is_primitive() const { 
        return static_cast<uint8_t>(tag_) >= static_cast<uint8_t>(AlgebraicTypeTag::String);
    }
    
    // -------------------------------------------------------------------------
    // DATA ACCESSOR METHODS
    // -------------------------------------------------------------------------
    
    uint32_t as_ref() const {
        if (!is_ref()) std::abort(); // Type is not a Ref
        return std::get<uint32_t>(data_);
    }
    
    const SumTypeSchema& as_sum() const {
        if (!is_sum()) std::abort(); // Type is not a Sum
        return *std::get<std::unique_ptr<SumTypeSchema>>(data_);
    }
    
    const ProductType& as_product() const {
        if (!is_product()) std::abort(); // Type is not a Product
        return *std::get<std::unique_ptr<ProductType>>(data_);
    }
    
    const ArrayType& as_array() const {
        if (!is_array()) std::abort(); // Type is not an Array
        return *std::get<std::unique_ptr<ArrayType>>(data_);
    }
    
    // -------------------------------------------------------------------------
    // COPY METHOD (SIMPLIFIED WITH VISITOR PATTERN)
    // -------------------------------------------------------------------------
    
    std::unique_ptr<AlgebraicType> copy() const {
        return std::visit([this](const auto& data) -> std::unique_ptr<AlgebraicType> {
            using DataT = std::decay_t<decltype(data)>;
            
            if constexpr (std::is_same_v<DataT, std::monostate>) {
                // Primitive types
                return std::make_unique<AlgebraicType>(tag_, std::monostate{});
            } else if constexpr (std::is_same_v<DataT, uint32_t>) {
                // Ref types
                return std::make_unique<AlgebraicType>(Ref(data));
            } else if constexpr (std::is_same_v<DataT, std::unique_ptr<SumTypeSchema>>) {
                // Sum types
                std::vector<SumTypeVariant> new_variants;
                new_variants.reserve(data->variants.size());
                for (const auto& variant : data->variants) {
                    new_variants.push_back(variant);  // Uses copy constructor
                }
                return std::make_unique<AlgebraicType>(AlgebraicType(
                    AlgebraicTypeTag::Sum,
                    std::make_unique<SumTypeSchema>(std::move(new_variants))
                ));
            } else if constexpr (std::is_same_v<DataT, std::unique_ptr<ProductType>>) {
                // Product types
                std::vector<ProductTypeElement> new_elements;
                new_elements.reserve(data->elements.size());
                for (const auto& elem : data->elements) {
                    new_elements.push_back(elem);  // Uses copy constructor
                }
                return std::make_unique<AlgebraicType>(AlgebraicType(
                    AlgebraicTypeTag::Product,
                    std::make_unique<ProductType>(std::move(new_elements))
                ));
            } else if constexpr (std::is_same_v<DataT, std::unique_ptr<ArrayType>>) {
                // Array types
                return std::make_unique<AlgebraicType>(AlgebraicType(
                    AlgebraicTypeTag::Array,
                    std::make_unique<ArrayType>(*data)  // Uses copy constructor
                ));
            }
        }, data_);
    }

private:
    // Helper methods for Option factory
    static AlgebraicType create_unit_option() {
        std::vector<SumTypeVariant> variants;
        variants.emplace_back("some", Unit());
        variants.emplace_back("none", Unit());
        return make_sum(std::make_unique<SumTypeSchema>(std::move(variants)));
    }
    
    static AlgebraicType create_typed_option(uint32_t some_type_ref) {
        std::vector<SumTypeVariant> variants;
        variants.emplace_back("some", Ref(some_type_ref));
        variants.emplace_back("none", Unit());
        return make_sum(std::make_unique<SumTypeSchema>(std::move(variants)));
    }
};

// ============================================================================
// ALGEBRAIC TYPE TRAITS
// ============================================================================

/**
 * Trait for getting the AlgebraicType of a C++ type.
 * Specialized for primitive and container types.
 * 
 * @example
 * auto type = algebraic_type_of<int32_t>::get();
 * auto array_type = algebraic_type_of<std::vector<int32_t>>::get();
 */
template<typename T>
struct algebraic_type_of {
    static AlgebraicType get();
};

// Helper macro to reduce repetitive primitive type specializations
#define SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE(cpp_type, tag_value) \
    template<> struct algebraic_type_of<cpp_type> { \
        static AlgebraicType get() { return AlgebraicType::primitive<AlgebraicTypeTag::tag_value>(); } \
    }

// Primitive type specializations
SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE(bool, Bool);
SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE(char, U8);
SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE(int8_t, I8);
SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE(int16_t, I16);
SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE(int32_t, I32);
SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE(int64_t, I64);
SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE(uint8_t, U8);
SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE(uint16_t, U16);
SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE(uint32_t, U32);
SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE(uint64_t, U64);
SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE(float, F32);
SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE(double, F64);
SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE(std::string, String);

#undef SPACETIMEDB_DEFINE_ALGEBRAIC_TYPE

// Container type specializations (properly implemented, no TODOs)
template<typename T> 
struct algebraic_type_of<std::vector<T>> {
    static AlgebraicType get() {
        return AlgebraicType::Array(algebraic_type_of<T>::get());
    }
};

template<typename T> 
struct algebraic_type_of<std::optional<T>> {
    static AlgebraicType get() {
        // Create proper Option type with the inner type
        AlgebraicType inner_type = algebraic_type_of<T>::get();
        
        std::vector<SumTypeVariant> variants;
        variants.emplace_back("some", std::move(inner_type));
        variants.emplace_back("none", AlgebraicType::Unit());
        
        return AlgebraicType::make_sum(std::make_unique<SumTypeSchema>(std::move(variants)));
    }
};

// ============================================================================
// INLINE IMPLEMENTATIONS
// ============================================================================

// ProductTypeElement implementations
inline ProductTypeElement::ProductTypeElement(std::optional<std::string> n, AlgebraicType type)
    : name(std::move(n)), algebraic_type(std::make_unique<AlgebraicType>(std::move(type))) {}

inline ProductTypeElement::ProductTypeElement(const ProductTypeElement& other)
    : name(other.name), algebraic_type(deep_copy_ptr(other.algebraic_type)) {}

inline ProductTypeElement& ProductTypeElement::operator=(const ProductTypeElement& other) {
    if (this != &other) {
        name = other.name;
        algebraic_type = deep_copy_ptr(other.algebraic_type);
    }
    return *this;
}

// SumTypeVariant implementations
inline SumTypeVariant::SumTypeVariant(std::string n, AlgebraicType type)
    : name(std::move(n)), algebraic_type(std::make_unique<AlgebraicType>(std::move(type))) {}

inline SumTypeVariant::SumTypeVariant(const SumTypeVariant& other)
    : name(other.name), algebraic_type(deep_copy_ptr(other.algebraic_type)) {}

inline SumTypeVariant& SumTypeVariant::operator=(const SumTypeVariant& other) {
    if (this != &other) {
        name = other.name;
        algebraic_type = deep_copy_ptr(other.algebraic_type);
    }
    return *this;
}

// ArrayType implementations
inline ArrayType::ArrayType(AlgebraicType elem_type)
    : element_type(std::make_unique<AlgebraicType>(std::move(elem_type))) {}

inline ArrayType::ArrayType(const ArrayType& other)
    : element_type(deep_copy_ptr(other.element_type)) {}

inline ArrayType& ArrayType::operator=(const ArrayType& other) {
    if (this != &other) {
        element_type = deep_copy_ptr(other.element_type);
    }
    return *this;
}

} // namespace SpacetimeDb::bsatn

#endif // SPACETIMEDB_BSATN_ALGEBRAIC_TYPE_H