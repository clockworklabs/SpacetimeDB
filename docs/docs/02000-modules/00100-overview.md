---
title: Overview
slug: /new-modules
---

# Modules - Overview

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

A **module** is a collection of functions and schema definitions, which can be written in TypeScript, C# or Rust. Modules define the structure of the [database](/databases) and the server-side logic that processes and handles client requests. 

The logic is contained within hree categories of server-side functions: [reducers](/functions/reducers) (transactional state changes), [procedures](/procedures) (functions with external capabilities), and [views](/functions/views) (read-only queries). Modules are administered using the `spacetime` CLI tool.

## Module vs Database

It's important to understand the distinction:

- A **module** is the code you write; it defines your schema (tables) and business logic (reducers, procedures, and views). Modules are compiled and deployed to SpacetimeDB. Rust and C# modules compile to WebAssembly, while TypeScript modules run on V8.
- A **database** is a *running instance* of a module; it has the module’s schema and logic, plus actual stored data.

You can deploy the same module to multiple databases (e.g. separate environments for testing, staging, production), each with its own independent data. When you update your module code and re-publish, SpacetimeDB will update the database’s schema/logic — the existing data remains (though for complicated schema changes you may need to handle migrations carefully).

## What's in a Module?

A module contains:

- **[Tables](/tables)** - Define your data structure and storage.
- **[Reducers](/functions/reducers)** - Server-side functions that modify your data transactionally.
- **[Procedures](/procedures)** - Functions that can perform external operations like HTTP requests and return results.
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

## Next Steps

Continue reading to learn how to:

- [Create a new module project](/new-modules/creating)
- [Build and publish your module](/new-modules/building-publishing)