#ifndef SPACETIMEDB_BSATN_MONOSTATE_TRAITS_H
#define SPACETIMEDB_BSATN_MONOSTATE_TRAITS_H

#include <variant>
#include "traits.h"
#include "writer.h"
#include "reader.h"
#include "algebraic_type.h"

namespace SpacetimeDb {
namespace bsatn {

/**
 * BSATN traits specialization for std::monostate
 * 
 * std::monostate represents a unit type (empty product with 0 fields)
 * This is equivalent to Rust's () unit type.
 * 
 * Serialization: Writes nothing (0 bytes)
 * Deserialization: Reads nothing, returns std::monostate{}
 * Algebraic type: Product with 0 fields (unit type)
 */
template<>
struct bsatn_traits<std::monostate> {
    static void serialize(Writer&, const std::monostate&) {
        // Unit type serializes to nothing (0 bytes)
        // This matches BSATN spec for empty products
    }
    
    static std::monostate deserialize(Reader&) {
        // Unit type deserializes from nothing
        // Just return a default-constructed monostate
        return std::monostate{};
    }
    
    static AlgebraicType algebraic_type() {
        // Unit is represented as an empty product (struct with 0 fields)
        // This matches how Rust's () is serialized
        return AlgebraicType::make_product({});
    }
};

} // namespace bsatn
} // namespace SpacetimeDb

#endif // SPACETIMEDB_BSATN_MONOSTATE_TRAITS_H