---
title: Clients
slug: /clients
---

The SpacetimeDB Client SDKs provide a comprehensive interface for building applications that connect to SpacetimeDB [databases](./00100-databases.md). Client applications can query data, invoke server-side functions, and receive real-time updates as the database state changes.

## Available SDKs

SpacetimeDB provides client SDKs for multiple languages:

- [Rust](./00600-clients/00500-rust-reference.md) - [(Quickstart)](../00100-intro/00200-quickstarts/00500-rust.md)
- [C#](./00600-clients/00600-csharp-reference.md) - [(Quickstart)](../00100-intro/00200-quickstarts/00600-c-sharp.md)
- [TypeScript](./00600-clients/00700-typescript-reference.md) - [(Quickstart)](../00100-intro/00200-quickstarts/00400-typescript.md)
- [Unreal](./00600-clients/00800-unreal-reference.md) - [(Tutorial)](../00100-intro/00300-tutorials/00400-unreal-tutorial/index.md)

## Getting Started

To build a client application with SpacetimeDB:

1. **[Generate client bindings](./00600-clients/00200-codegen.md)** - Use `spacetime generate` to create type-safe bindings for your [database](./00100-databases.md)
2. **[Connect to your database](./00600-clients/00300-connection.md)** - Establish a WebSocket connection to SpacetimeDB
3. **[Use the SDK API](./00600-clients/00400-sdk-api.md)** - Subscribe to data, invoke functions, and register callbacks

## Core Capabilities

### Connection Management

The SDKs handle establishing and maintaining WebSocket connections to SpacetimeDB servers. Connections support authentication via tokens (for example, from [SpacetimeAuth](./00500-authentication/00100-spacetimeauth/index.md)) and provide lifecycle callbacks for connect, disconnect, and error events.

See [Connecting to SpacetimeDB](./00600-clients/00300-connection.md) for details.

### Client-Side Data Cache

Each client maintains a local cache of database rows through [subscriptions](./00400-subscriptions.md). Clients define which data they need using typed query builders (or raw SQL when needed), and SpacetimeDB automatically synchronizes changes to the subscribed data. The local cache can be queried without network round-trips, providing fast access to frequently-read data.

### Real-Time Updates

Clients receive automatic updates when subscribed data changes. The SDKs provide callbacks for observing:

- **Subscription updates** - When subscription queries are applied or fail
- **Row changes** - When rows are inserted, updated, or deleted in the local cache
- **Reducer invocations** - When [reducers](./00200-functions/00300-reducers/00300-reducers.md) run on the server
- **Procedure results** - When [procedures](./00200-functions/00400-procedures.md) are called the results are returned via a callback

### Invoking Server Functions

Clients can invoke server-side functions to modify data or perform operations:

- **[Reducers](./00200-functions/00300-reducers/00300-reducers.md)** - Transactional functions that modify database state
- **[Procedures](./00200-functions/00400-procedures.md)** - Functions that can perform external operations like HTTP requests (beta)

### Type Safety

The [generated client bindings](./00600-clients/00200-codegen.md) provide compile-time type safety between your client and server code. Table schemas, function signatures, and return types are all reflected in the generated code, catching errors before runtime.

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

1. **[Generate Client Bindings](./00600-clients/00200-codegen.md)** - Create type-safe interfaces from your module
2. **[Connect to SpacetimeDB](./00600-clients/00300-connection.md)** - Establish a connection and understand the lifecycle
3. **[Use the SDK API](./00600-clients/00400-sdk-api.md)** - Learn about subscriptions, reducers, and callbacks
4. **Language Reference** - Dive into language-specific details: [Rust](./00600-clients/00500-rust-reference.md), [C#](./00600-clients/00600-csharp-reference.md), [TypeScript](./00600-clients/00700-typescript-reference.md)

## Next Steps

- Follow a **Quickstart guide** [Rust](../00100-intro/00200-quickstarts/00500-rust.md), [C#](../00100-intro/00200-quickstarts/00600-c-sharp.md), or [TypeScript](../00100-intro/00200-quickstarts/00400-typescript.md) to build your first client
- Learn about [Databases](./00100-databases.md) to understand what you're connecting to
- Explore [Subscriptions](./00400-subscriptions.md) for efficient data synchronization
- Review [Reducers](./00200-functions/00300-reducers/00300-reducers.md) to understand server-side state changes
