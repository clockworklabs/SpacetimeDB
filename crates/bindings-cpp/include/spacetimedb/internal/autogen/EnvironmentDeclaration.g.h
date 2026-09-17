#pragma once
#include "EnvVarType.g.h"
namespace SpacetimeDB::Internal {
SPACETIMEDB_INTERNAL_PRODUCT_TYPE(EnvironmentDeclaration) {
    std::string name;
    EnvVarType ty;
    bool optional;
    void bsatn_serialize(::SpacetimeDB::bsatn::Writer& writer) const {
        ::SpacetimeDB::bsatn::serialize(writer, name);
        ::SpacetimeDB::bsatn::serialize(writer, ty);
        ::SpacetimeDB::bsatn::serialize(writer, optional);
    }
    SPACETIMEDB_PRODUCT_TYPE_EQUALITY(name, ty, optional)
};
}
