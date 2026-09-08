#include <spacetimedb/environment.h>
#include <cassert>
#include <type_traits>

int main() {
    static_assert(std::is_same_v<decltype(SpacetimeDB::Environment{}.get("UNDECLARED")), std::optional<std::string>>);
    assert(SpacetimeDB::Internal::environment_declarations().empty());
}
