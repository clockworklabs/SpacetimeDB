---
title: Overview
slug: /sdks
---

The SpacetimeDB Client SDKs provide a comprehensive interface for building applications that connect to SpacetimeDB [databases](/databases). Client applications can query data, invoke server-side functions, and receive real-time updates as the database state changes.

## Available SDKs

SpacetimeDB provides client SDKs for multiple languages:

- [Rust](/sdks/rust) - [(Quickstart)](/docs/quickstarts/rust)
- [C#](/sdks/c-sharp) - [(Quickstart)](/docs/quickstarts/c-sharp)
- [TypeScript](/sdks/typescript) - [(Quickstart)](/docs/quickstarts/typescript)
- [Unreal](/sdks/unreal) - [(Tutorial)](/tutorials/unreal)

## Getting Started

To build a client application with SpacetimeDB:

1. **[Generate client bindings](/sdks/codegen)** - Use `spacetime generate` to create type-safe bindings for your [database](/databases)
2. **[Connect to your database](/sdks/connection)** - Establish a WebSocket connection to SpacetimeDB
3. **[Use the SDK API](/sdks/api)** - Subscribe to data, invoke functions, and register callbacks

## Core Capabilities

### Connection Management

The SDKs handle establishing and maintaining WebSocket connections to SpacetimeDB servers. Connections support authentication via tokens (for example, from [SpacetimeAuth](./00500-authentication/00100-spacetimeauth/index.md)) and provide lifecycle callbacks for connect, disconnect, and error events.

See [Connecting to SpacetimeDB](/sdks/connection) for details.

### Client-Side Data Cache

Each client maintains a local cache of database rows through [subscriptions](/subscriptions). Clients define which data they need using SQL queries, and SpacetimeDB automatically synchronizes changes to the subscribed data. The local cache can be queried without network round-trips, providing fast access to frequently-read data.

### Real-Time Updates

Clients receive automatic updates when subscribed data changes. The SDKs provide callbacks for observing:

- **Subscription updates** - When subscription queries are applied or fail
- **Row changes** - When rows are inserted, updated, or deleted in the local cache
- **Reducer invocations** - When [reducers](/functions/reducers) run on the server
- **Procedure results** - When [procedures](/functions/procedures) are called the results are returned via a callback

### Invoking Server Functions

Clients can invoke server-side functions to modify data or perform operations:

- **[Reducers](/functions/reducers)** - Transactional functions that modify database state
- **[Procedures](/functions/procedures)** - Functions that can perform external operations like HTTP requests (beta)

### Type Safety

The [generated client bindings](/sdks/codegen) provide compile-time type safety between your client and server code. Table schemas, function signatures, and return types are all reflected in the generated code, catching errors before runtime.

## Choosing a Language

When selecting a language for your client application, consider these factors:

### Team Expertise

Choose a language your development team is comfortable with to maximize productivity and reduce development time.

### Application Type

- **Web applications** - TypeScript integrates seamlessly with browser and Node.js environments
- **Desktop applications** - Rust or C# depending on your platform and requirements
- **Performance-critical applications** - Rust offers the best performance and memory efficiency
- **Unity games** - C# is required for Unity development
- **Unreal games** - C++ and Blueprint are both supported for Unreal clients

### Platform and Ecosystem

Each language has its own ecosystem of libraries and tools. If your application depends on specific libraries or frameworks, that may influence your choice.

The functionality of the SDKs remains consistent across languages, so transitioning between them primarily involves syntax changes rather than architectural changes. You can even use multiple languages in the same project - for example, C# for a Unity game client and TypeScript for a web administration panel.

## Learning Path

New to SpacetimeDB client development? Follow this progression:

1. **[Generate Client Bindings](/sdks/codegen)** - Create type-safe interfaces from your module
2. **[Connect to SpacetimeDB](/sdks/connection)** - Establish a connection and understand the lifecycle
3. **[Use the SDK API](/sdks/api)** - Learn about subscriptions, reducers, and callbacks
4. **Language Reference** - Dive into language-specific details: [Rust](/sdks/rust), [C#](/sdks/c-sharp), [TypeScript](/sdks/typescript)

## Next Steps

- Follow a **Quickstart guide** [Rust](/quickstarts/rust), [C#](/quickstarts/c-sharp), or [TypeScript](/quickstarts/typescript) to build your first client
- Learn about [Databases](/databases) to understand what you're connecting to
- Explore [Subscriptions](/subscriptions) for efficient data synchronization
- Review [Reducers](/functions/reducers) to understand server-side state changes
