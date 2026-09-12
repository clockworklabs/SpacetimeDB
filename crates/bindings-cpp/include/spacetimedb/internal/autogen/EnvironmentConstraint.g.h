#pragma once
#include "../autogen_base.h"
#include <string>
#include <vector>
namespace SpacetimeDB::Internal {
SPACETIMEDB_INTERNAL_TAGGED_ENUM(EnvironmentConstraint, std::monostate, std::string, std::vector<std::string>)
}
