#pragma once
#include "EnvironmentConstraint.g.h"
namespace SpacetimeDB::Internal {
SPACETIMEDB_INTERNAL_PRODUCT_TYPE(EnvironmentDeclaration) {
    std::string name;
    EnvironmentConstraint constraint;
    bool optional;
    void bsatn_serialize(::SpacetimeDB::bsatn::Writer& writer) const {
        ::SpacetimeDB::bsatn::serialize(writer, name);
        ::SpacetimeDB::bsatn::serialize(writer, constraint);
        ::SpacetimeDB::bsatn::serialize(writer, optional);
    }
    SPACETIMEDB_PRODUCT_TYPE_EQUALITY(name, constraint, optional)
};
}
