// Implementation for AlgebraicType to handle circular dependencies
#include "spacetimedb/internal/autogen/AlgebraicType.g.h"
#include "spacetimedb/internal/autogen/SumType.g.h"
#include "spacetimedb/internal/autogen/ProductType.g.h"
#include "spacetimedb/internal/autogen/SumTypeVariant.g.h"
#include "spacetimedb/internal/autogen/ProductTypeElement.g.h"

namespace SpacetimeDB::Internal {

// Default constructor - String type
AlgebraicType::AlgebraicType() : tag_(Tag::String), data_(std::in_place_index<4>, std::monostate{}) {}

// Constructor for primitive types using in_place_index
AlgebraicType::AlgebraicType(Tag primitive_tag) : tag_(primitive_tag) {
    switch (primitive_tag) {
        case Tag::Ref: 
            data_.emplace<0>(uint32_t{0}); 
            break;
        case Tag::Sum: 
            data_.emplace<1>(std::unique_ptr<SpacetimeDB::Internal::SumType>{}); 
            break;
        case Tag::Product: 
            data_.emplace<2>(std::unique_ptr<SpacetimeDB::Internal::ProductType>{}); 
            break;
        case Tag::Array: 
            data_.emplace<3>(std::unique_ptr<SpacetimeDB::Internal::AlgebraicType>{}); 
            break;
        default: 
            // All primitive types use index 4 with monostate
            data_.emplace<4>(std::monostate{}); 
            break;
    }
}

// Copy constructor - must handle unique_ptr deep copy manually
AlgebraicType::AlgebraicType(const AlgebraicType& other) : tag_(other.tag_) {
    // Deep copy for pointer types, direct copy for others
    switch (tag_) {
        case Tag::Ref:
            data_.emplace<0>(std::get<0>(other.data_));
            break;
        case Tag::Sum: {
            auto& ptr = std::get<1>(other.data_);
            if (ptr) {
                data_.emplace<1>(std::make_unique<SpacetimeDB::Internal::SumType>(*ptr));
            } else {
                data_.emplace<1>(std::unique_ptr<SpacetimeDB::Internal::SumType>{});
            }
            break;
        }
        case Tag::Product: {
            auto& ptr = std::get<2>(other.data_);
            if (ptr) {
                data_.emplace<2>(std::make_unique<SpacetimeDB::Internal::ProductType>(*ptr));
            } else {
                data_.emplace<2>(std::unique_ptr<SpacetimeDB::Internal::ProductType>{});
            }
            break;
        }
        case Tag::Array: {
            auto& ptr = std::get<3>(other.data_);
            if (ptr) {
                data_.emplace<3>(std::make_unique<SpacetimeDB::Internal::AlgebraicType>(*ptr));
            } else {
                data_.emplace<3>(std::unique_ptr<SpacetimeDB::Internal::AlgebraicType>{});
            }
            break;
        }
        default: 
            // All primitive types use index 4 with monostate
            data_.emplace<4>(std::monostate{});
            break;
    }
}

// Assignment operator
AlgebraicType& AlgebraicType::operator=(const AlgebraicType& other) {
    if (this != &other) {
        AlgebraicType temp(other);
        std::swap(tag_, temp.tag_);
        std::swap(data_, temp.data_);
    }
    return *this;
}

// Template setters by index
template<size_t Index, typename T>
void AlgebraicType::set(T&& value) {
    // For primitive types, always use index 4
    if (Index >= 4) {
        tag_ = static_cast<Tag>(Index);
        data_.template emplace<4>(std::monostate{});
    } else {
        tag_ = static_cast<Tag>(Index);
        data_.template emplace<Index>(std::forward<T>(value));
    }
}

// BSATN serialization (only serialization needed for Internal types)
void AlgebraicType::bsatn_serialize(::SpacetimeDB::bsatn::Writer& writer) const {
    writer.write_u8(static_cast<uint8_t>(tag_));
    //fprintf(stdout, "DEBUG: AlgebraicType serializing tag=%d START\n", static_cast<int>(tag_));
    
    // Serialize data based on tag
    switch (tag_) {
        case Tag::Ref:
            // Write the type reference
            writer.write_u32_le(std::get<0>(data_));
            //fprintf(stdout, "  Ref to type %u\n", std::get<0>(data_));
            break;
            
        case Tag::Sum: {
            // Serialize the SumType
            const auto& sum_ptr = std::get<1>(data_);
            if (sum_ptr) {
                sum_ptr->bsatn_serialize(writer);
            } else {
                // Write empty sum (0 variants)
                writer.write_u32_le(0);
            }
            break;
        }
        
        case Tag::Product: {
            // Serialize the ProductType
            const auto& product_ptr = std::get<2>(data_);
            if (product_ptr) {
                //fprintf(stdout, "  Product with %zu elements\n", product_ptr->elements.size());
                product_ptr->bsatn_serialize(writer);
            } else {
                // Write empty product (0 elements)
                //fprintf(stdout, "  Empty Product (nullptr) - writing 0 elements\n");
                writer.write_u32_le(0);
            }
            break;
        }
        
        case Tag::Array: {
            // Serialize the element type
            const auto& elem_ptr = std::get<3>(data_);
            if (elem_ptr) {
                elem_ptr->bsatn_serialize(writer);
            } else {
                // Write String as default element type
                writer.write_u8(static_cast<uint8_t>(Tag::String));
            }
            break;
        }
        
        default:
            // Primitive types - no additional data to write
            //fprintf(stdout, "DEBUG: AlgebraicType primitive tag=%d END\n", static_cast<int>(tag_));
            break;
    }
    //fprintf(stdout, "DEBUG: AlgebraicType serializing tag=%d COMPLETE\n", static_cast<int>(tag_));
}

// Equality
bool AlgebraicType::operator==(const AlgebraicType& other) const {
    return tag_ == other.tag_ && data_ == other.data_;
}

// Explicit template instantiations for all possible indices
template void AlgebraicType::set<0, uint32_t>(uint32_t&&);
template void AlgebraicType::set<0, uint32_t&>(uint32_t&);  // For lvalue references
template void AlgebraicType::set<1, std::unique_ptr<SumType>>(std::unique_ptr<SumType>&&);
template void AlgebraicType::set<2, std::unique_ptr<ProductType>>(std::unique_ptr<ProductType>&&);
template void AlgebraicType::set<3, std::unique_ptr<AlgebraicType>>(std::unique_ptr<AlgebraicType>&&);
template void AlgebraicType::set<4, std::monostate>(std::monostate&&);

} // namespace SpacetimeDB::Internal