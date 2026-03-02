#include <spacetimedb.h>
#include <optional>
#include <vector>
#include <cmath>

using namespace SpacetimeDB;

// Tables matching Rust `sdk-test-connect-disconnect`

struct Connected {
	Identity identity;
};
SPACETIMEDB_STRUCT(Connected, identity)
SPACETIMEDB_TABLE(Connected, connected, Public)

struct Disconnected {
	Identity identity;
};
SPACETIMEDB_STRUCT(Disconnected, identity)
SPACETIMEDB_TABLE(Disconnected, disconnected, Public)

// Reducers: client lifecycle callbacks

SPACETIMEDB_CLIENT_CONNECTED(identity_connected, ReducerContext ctx) {
	ctx.db[connected].insert(Connected{ ctx.sender });
	return Ok();
}

SPACETIMEDB_CLIENT_DISCONNECTED(identity_disconnected, ReducerContext ctx) {
	ctx.db[disconnected].insert(Disconnected{ ctx.sender });
	return Ok();
}
