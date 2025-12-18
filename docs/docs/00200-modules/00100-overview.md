---
title: Overview
slug: /modules
---

# Modules - Overview

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

A **module** is a collection of functions and schema definitions, which can be written in TypeScript, C# or Rust. Modules define the structure of the [database](/databases) and the server-side logic that processes and handles client requests.

The logic is contained within three categories of server-side functions: [reducers](/functions/reducers) (transactional state changes), [procedures](/functions/procedures) (functions with external capabilities), and [views](/functions/views) (read-only queries). Modules are administered using the `spacetime` CLI tool.

## Module vs Database

It's important to understand the distinction:

- A **module** is the code you write; it defines your schema (tables) and business logic (reducers, procedures, and views). Modules are compiled and deployed to SpacetimeDB. Rust and C# modules compile to WebAssembly, while TypeScript modules run on V8.
- A **database** is a *running instance* of a module; it has the module’s schema and logic, plus actual stored data.

You can deploy the same module to multiple databases (e.g. separate environments for testing, staging, production), each with its own independent data. When you update your module code and re-publish, SpacetimeDB will update the database’s schema/logic — the existing data remains (though for complicated schema changes you may need to handle migrations carefully).

## What's in a Module?

A module contains:

- **[Tables](/tables)** - Define your data structure and storage.
- **[Reducers](/functions/reducers)** - Server-side functions that modify your data transactionally.
- **[Procedures](/functions/procedures)** - Functions that can perform external operations like HTTP requests and return results.
- **[Views](/functions/views)** - Read-only computed queries over your data.

## Supported Languages

SpacetimeDB modules can be written in multiple languages:

<Tabs groupId="server-language" queryString>
<TabItem value="rust" label="Rust">

Rust is fully supported for server modules. Rust is a great choice for performance-critical applications.

- The Rust Module SDK docs are [hosted on docs.rs](https://docs.rs/spacetimedb/latest/spacetimedb/).
- [Rust Quickstart Guide](/quickstarts/rust)

</TabItem>
<TabItem value="csharp" label="C#">

C# is fully supported for server modules. C# is an excellent choice for developers using Unity or .NET.

- [C# Quickstart Guide](/quickstarts/c-sharp)

</TabItem>
<TabItem value="typescript" label="TypeScript">

TypeScript is fully supported for server modules. TypeScript is ideal for developers familiar with JavaScript/Node.js.

- [TypeScript Quickstart Guide](/quickstarts/typescript)

</TabItem>
</Tabs>

## Learning Path

### Getting Started

If you're new to SpacetimeDB modules, follow this recommended learning path:

1. **[Create Your First Module](/modules/creating)** - Set up a new module project with `spacetime init` or `spacetime dev`
2. **[Build and Publish](/modules/building-publishing)** - Learn how to compile and deploy your module
3. **[Define Tables](/tables)** - Structure your data with tables, columns, and indexes
4. **[Write Reducers](/functions/reducers)** - Create transactional functions that modify your database
5. **[Connect a Client](/sdks)** - Build a client application that connects to your database

### Core Concepts

Once you have the basics down, explore these essential topics:

- **[Logging](/modules/logging)** - Debug and monitor your module with logging
- **[Error Handling](/functions/reducers/error-handling)** - Handle errors gracefully in reducers
- **[Lifecycle Reducers](/functions/reducers/lifecycle)** - Respond to system events like initialization and client connections
- **[Row-Level Security](/modules/rls)** - Control what data clients can access
- **[Automatic Migrations](/databases/automatic-migrations)** - Understand how schema changes work

### Advanced Features

Ready to level up? Dive into these advanced capabilities:

- **[Procedures](/functions/procedures)** - Make HTTP requests and interact with external services
- **[Views](/functions/views)** - Create computed, subscribable queries
- **[Scheduled Tables](/tables/scheduled-tables)** - Schedule reducers to run at specific times
- **[Incremental Migrations](/databases/incremental-migrations)** - Handle complex schema changes
- **[SQL Queries](/databases/sql)** - Query your database with SQL

### Deployment

When you're ready to go live:

- **[Deploy to MainCloud](/modules/deploying/maincloud)** - Host your database on SpacetimeDB's managed service
- **[Self-Hosting](/modules/deploying/self-hosting)** - Run your own SpacetimeDB instance

## Next Steps

Continue reading to learn how to:

- [Create a new module project](/modules/creating)
- [Build and publish your module](/modules/building-publishing)