#include <spacetimedb/environment.h>
#include <spacetimedb/reducer_context.h>
std::string from_another_translation_unit() {
    return SpacetimeDB::Environment{}.FOOBAR();
}
static_assert(std::is_same_v<decltype(std::declval<SpacetimeDB::ReducerContext>().env.FOOBAR()), std::string>);
