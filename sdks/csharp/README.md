# SpacetimeDB SDK for C#

The SpacetimeDB SDK is a software development kit (SDK) designed to simplify the interaction with SpacetimeDB server modules.

## Key Features

### Connection Management

The SDK simplifies the process of establishing and managing connections to the SpacetimeDB module. Developers can establish secure WebSocket connections, enabling real-time communication with the module.

### Local Client Cache

By subscribing to a set of queries, the SDK will keep a local cache of rows that match the subscribed queries. SpacetimeDB generates C# files that allow you to iterate throught these tables and filter on specific columns.

### Transaction and Row Update Events

Register for transaction and row update events.

Full documentation can be found on the [SpacetimeDB](spacetimedb.com) website.
