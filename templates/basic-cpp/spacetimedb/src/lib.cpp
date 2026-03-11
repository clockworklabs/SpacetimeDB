#include "spacetimedb.h"

using namespace SpacetimeDB;

// Define a simple table
struct Person {
    std::string name;
};
SPACETIMEDB_STRUCT(Person, name)
SPACETIMEDB_TABLE(Person, person, Public)

// Called when the module is initially published
SPACETIMEDB_INIT(init, ReducerContext ctx)
{
    // Module initialization logic goes here
    return Ok();
}

// Called every time a new client connects
SPACETIMEDB_CLIENT_CONNECTED(identity_connected, ReducerContext ctx)
{
    // Client connection logic goes here
    return Ok();
}

// Called every time a client disconnects  
SPACETIMEDB_CLIENT_DISCONNECTED(identity_disconnected, ReducerContext ctx)
{
    // Client disconnection logic goes here
    return Ok();
}

// Add a person to the table
SPACETIMEDB_REDUCER(add, ReducerContext ctx, std::string name)
{
    ctx.db[person].insert(Person{name});
    return Ok();
}

// Say hello to everyone in the table
SPACETIMEDB_REDUCER(say_hello, ReducerContext ctx)
{
    auto table = ctx.db[person];
    for (const auto& person : table) {
        LOG_INFO("Hello, " + person.name + "!");
    }
    LOG_INFO("Hello, World!");
    return Ok();
}