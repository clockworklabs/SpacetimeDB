---
title: Server Module Overview
navTitle: Overview
---

Server modules are the core of a SpacetimeDB application. They define the structure of the database and the server-side logic that processes and handles client requests. These functions are called reducers and are transactional, meaning they ensure data consistency and integrity. Reducers can perform operations such as inserting, updating, and deleting data in the database.

In the following sections, we'll cover the basics of server modules and how to create and deploy them.

## Supported Languages

### Rust

As of SpacetimeDB 0.6, Rust is the only fully supported language for server modules. Rust is a great option for server modules because it is fast, safe, and has a small runtime.

- [Rust Module Reference](/docs/module/rust-reference)
- [Rust Module Quickstart Guide](/docs/module/rust-quickstart)

### C#

We have C# support available in experimental status. C# can be a good choice for developers who are already using Unity or .net for their client applications.

- [C# Module Reference](/docs/module/c-sharp-reference)
- [C# Module Quickstart Guide](/docs/module/c-sharp-quickstart)

### Coming Soon

We have plans to support additional languages in the future.

- Python
- Typescript
- C++
- Lua
