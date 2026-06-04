# SpacetimeDB

> SpacetimeDB is a database that lets you write your entire application as a database module. Server logic runs inside the database as WebAssembly. Clients subscribe to queries and get real-time updates over WebSocket. No separate server needed.


## docs

Installation

- [Getting Started](/docs/): Installation

### bsatn

The Spacetime Algebraic Type Notation binary (BSATN) format defines

- [BSATN Data Format](/docs/bsatn): The Spacetime Algebraic Type Notation binary (BSATN) format defines

### cli-reference

This document contains the help content for the spacetime command-line program.

- [Command-Line Help for spacetime](/docs/cli-reference): This document contains the help content for the spacetime command-line program.
- [spacetime.json Configuration File](/docs/cli-reference/spacetime-json): The spacetime.json file defines project-level configuration for the SpacetimeDB CLI. It eliminates repetitive CLI flags and enables multi-target workflows such as publishing multiple databases or generating bindings for multiple languages from a single project.
- [Standalone Configuration](/docs/cli-reference/standalone-config): A local database instance (as started by spacetime start) can be configured in /config.toml, where {data-dir} is the database's data directory. This directory is printed when you run spacetime start:

### clients

The SpacetimeDB Client SDKs provide a comprehensive interface for building applications that connect to SpacetimeDB databases. Client applications can query data, invoke server-side functions, and receive real-time updates as the database state changes.

- [Clients](/docs/clients): The SpacetimeDB Client SDKs provide a comprehensive interface for building applications that connect to SpacetimeDB databases. Client applications can query data, invoke server-side functions, and receive real-time updates as the database state changes.
- [SDK API Overview](/docs/clients/api): The SpacetimeDB client SDKs provide a comprehensive API for interacting with your database. After generating client bindings and establishing a connection, you can query data, invoke server functions, and observe real-time changes.
- [C# Reference](/docs/clients/c-sharp): The SpacetimeDB client for C# contains all the tools you need to build native clients for SpacetimeDB modules using C#.
- [Generating Client Bindings](/docs/clients/codegen): Before you can interact with a SpacetimeDB database from a client application, you must generate client bindings for your module. These bindings create type-safe interfaces that allow your client to query tables, invoke reducers, call procedures, and subscribe to tables, and/or views.
- [Connecting to SpacetimeDB](/docs/clients/connection): After generating client bindings for your module, you can establish a connection to your SpacetimeDB database from your client application. The DbConnection type provides a persistent WebSocket connection that enables real-time communication with the server.
- [Rust Reference](/docs/clients/rust): The SpacetimeDB client SDK for Rust contains all the tools you need to build native clients for SpacetimeDB modules using Rust.
- [Subscriptions](/docs/clients/subscriptions): Subscriptions replicate database rows to your client in real-time. When you subscribe to a query, SpacetimeDB sends you the matching rows immediately and then pushes updates whenever those rows change.
- [Subscription Semantics](/docs/clients/subscriptions/semantics): This document describes the subscription semantics maintained by the SpacetimeDB host over WebSocket connections. These semantics outline message ordering guarantees, subscription handling, transaction updates, and client cache consistency.
- [TypeScript Reference](/docs/clients/typescript): The SpacetimeDB client SDK for TypeScript contains all the tools you need to build clients for SpacetimeDB modules using Typescript, either in the browser or with NodeJS.
- [Unreal Reference](/docs/clients/unreal): The SpacetimeDB client for Unreal Engine contains all the tools you need to build native clients for SpacetimeDB modules using C++ and Blueprint.

### core-concepts

This section covers the fundamental concepts you need to understand to build applications with SpacetimeDB.

- [Core Concepts](/docs/core-concepts): This section covers the fundamental concepts you need to understand to build applications with SpacetimeDB.
- [Authentication](/docs/core-concepts/authentication): SpacetimeDB modules are exposed to the open internet and anyone can connect to
- [Auth0](/docs/core-concepts/authentication/Auth0): This guilde will walk you through integrating Auth0 authentication with your SpacetimeDB application.
- [Clerk](/docs/core-concepts/authentication/Clerk): This guide will walk you through integrating Clerk authentication with your SpacetimeDB React application. You will configure a Clerk application, obtain a JWT from Clerk, and pass it to your SpacetimeDB connection as the authentication token.
- [Overview](/docs/core-concepts/authentication/spacetimeauth/): SpacetimeAuth is currently in beta, some features may not be available yet or
- [Configuring your project](/docs/core-concepts/authentication/spacetimeauth/configuring-a-project): SpacetimeAuth is currently in beta, some features may not be available yet or may change in the future. You might encounter bugs or issues while using the service. Please report any problems you encounter to help us improve SpacetimeAuth.
- [Creating a project](/docs/core-concepts/authentication/spacetimeauth/creating-a-project): SpacetimeAuth is currently in beta, some features may not be available yet or may change in the future. You might encounter bugs or issues while using the service. Please report any problems you encounter to help us improve SpacetimeAuth.
- [React Integration](/docs/core-concepts/authentication/spacetimeauth/react-integration): SpacetimeAuth is currently in beta, some features may not be available yet or may change in the future. You might encounter bugs or issues while using the service. Please report any problems you encounter to help us improve SpacetimeAuth.
- [Testing](/docs/core-concepts/authentication/spacetimeauth/testing): SpacetimeAuth is currently in beta, some features may not be available yet or may change in the future. You might encounter bugs or issues while using the service. Please report any problems you encounter to help us improve SpacetimeAuth.
- [Using Auth Claims](/docs/core-concepts/authentication/usage): SpacetimeDB allows you to easily access authentication (auth) claims embedded in

### databases

A module is a collection of functions and schema definitions, which can be written in TypeScript, C#, Rust, or C++. Modules define the structure of your database and the server-side logic that processes and handles client requests.

- [The Database Module](/docs/databases): A module is a collection of functions and schema definitions, which can be written in TypeScript, C#, Rust, or C++. Modules define the structure of your database and the server-side logic that processes and handles client requests.
- [Automatic Migrations](/docs/databases/automatic-migrations): When you republish a module to an existing database using spacetime publish , SpacetimeDB attempts to automatically migrate your database schema to match the new module definition. This allows you to update your module code and redeploy without losing existing data, as long as the changes are compatible.
- [spacetime publish](/docs/databases/building-publishing): This guide covers how to build and publish your SpacetimeDB module.
- [Cheat Sheet](/docs/databases/cheat-sheet): Quick reference for SpacetimeDB module syntax across Rust, C#, and TypeScript.
- [spacetime dev](/docs/databases/developing): This guide covers how to create a new SpacetimeDB database module project.
- [Incremental Migrations](/docs/databases/incremental-migrations): SpacetimeDB does not provide built-in support for general schema-modifying migrations. It does, however, allow adding new tables, and changing reducers' definitions in arbitrary ways. It's possible to run general migrations using an external tool, but this is tedious, necessitates downtime, and imposes the requirement that you update all your clients at the same time as publishing your new module version.
- [Transactions and Atomicity](/docs/databases/transactions-atomicity): SpacetimeDB provides strong transactional guarantees for all database operations. Every reducer runs inside a database transaction, ensuring your data remains consistent and reliable even under concurrent load.

### functions

| Property / Characteristic | Reducers | Procedures | Views |

- [Functions](/docs/functions): | Property / Characteristic | Reducers | Procedures | Views |
- [Procedures](/docs/functions/procedures): A procedure is a function exported by a database, similar to a reducer.
- [Overview](/docs/functions/reducers): Reducers are functions that modify database state in response to client requests or system events. They are the only way to mutate tables in SpacetimeDB - all database changes must go through reducers.
- [Error Handling](/docs/functions/reducers/error-handling): Error Handling
- [Lifecycle Reducers](/docs/functions/reducers/lifecycle): Special reducers handle system events during the database lifecycle.
- [Reducer Context](/docs/functions/reducers/reducer-context): Every reducer receives a special context parameter as its first argument. This context provides read-write access to the database, information about the caller, and additional utilities like random number generation.
- [Views](/docs/functions/views): Views are read-only functions that compute and return results from your tables. Unlike reducers, views do not modify database state - they only query and return data. Views are useful for computing derived data, aggregations, or joining multiple tables before sending results to clients.

### how-to

- [Maincloud](/docs/how-to/deploy/maincloud): Maincloud is SpacetimeDB's fully managed serverless platform. It handles infrastructure, scaling, replication, and backups so you can focus on building your application. Maincloud scales to zero when your database is idle, so you only pay for what you use.
- [Self-hosting](/docs/how-to/deploy/self-hosting): This tutorial will guide you through setting up SpacetimeDB on an Ubuntu 24.04 server, securing it with HTTPS using Nginx and Let's Encrypt, and configuring a systemd service to keep it running.
- [Logging](/docs/how-to/logging): SpacetimeDB provides logging capabilities for debugging and monitoring your modules. Log messages are private to the database owner and are not visible to clients.
- [PostgreSQL Wire Protocol (PGWire)](/docs/how-to/pg-wire): SpacetimeDB supports the PostgreSQL wire protocol (PGWire),
- [Reject Client Connections](/docs/how-to/reject-client-connections): SpacetimeDB provides a way to disconnect a client during a client connection attempt.
- [Row Level Security](/docs/how-to/rls): Row Level Security is an experimental, unstable feature. The API may change or be removed in future releases.
- [Azure Self-Hosted VMs + Key Rotation & Key Vault](/docs/how-to/self-hosted-key-rotation): This guide explains how JWT signing key rotation works in self-hosted SpacetimeDB and how to avoid breaking spacetime publish during rotation.

### http

- [Authorization](/docs/http/authorization): Generating identities and tokens
- [/v1/database](/docs/http/database): The HTTP endpoints in /v1/database allow clients to interact with Spacetime databases in a variety of ways, including retrieving information, creating and deleting databases, invoking reducers and evaluating SQL queries.
- [/v1/identity](/docs/http/identity): The HTTP endpoints in /v1/identity allow clients to generate and manage Spacetime public identities and private tokens.

### intro

- [FAQ](/docs/intro/faq): General
- [Key Architecture](/docs/intro/key-architecture): Host
- [Language Support](/docs/intro/language-support): Server Database Modules
- [What is SpacetimeDB?](/docs/intro/what-is-spacetimedb): SpacetimeDB is a database that is also a server.
- [The Zen of SpacetimeDB](/docs/intro/zen): SpacetimeDB is built on 5 core principles. As you embrace these simple principles, you will find your troubles simply melt away. These principles guide both how we develop SpacetimeDB and how you should think about building applications with it.

### quickstarts

- [Angular Quickstart](/docs/quickstarts/angular): Get a SpacetimeDB Angular app running in under 5 minutes.
- [Browser Quickstart](/docs/quickstarts/browser): Get a SpacetimeDB app running in the browser with inline JavaScript.
- [Bun Quickstart](/docs/quickstarts/bun): Get a SpacetimeDB Bun app running in under 5 minutes.
- [C++ Quickstart](/docs/quickstarts/c-plus-plus): Get a SpacetimeDB C++ app running in under 5 minutes.
- [C# Quickstart](/docs/quickstarts/c-sharp): Get a SpacetimeDB C# app running in under 5 minutes.
- [Deno Quickstart](/docs/quickstarts/deno): Get a SpacetimeDB Deno app running in under 5 minutes.
- [Next.js Quickstart](/docs/quickstarts/nextjs): Get a SpacetimeDB Next.js app running in under 5 minutes.
- [Node.js Quickstart](/docs/quickstarts/nodejs): Get a SpacetimeDB Node.js app running in under 5 minutes.
- [Nuxt Quickstart](/docs/quickstarts/nuxt): Get a SpacetimeDB Nuxt app running in under 5 minutes.
- [React Quickstart](/docs/quickstarts/react): Get a SpacetimeDB React app running in under 5 minutes.
- [Remix Quickstart](/docs/quickstarts/remix): Get a SpacetimeDB Remix app running in under 5 minutes.
- [Rust Quickstart](/docs/quickstarts/rust): Get a SpacetimeDB Rust app running in under 5 minutes.
- [Svelte Quickstart](/docs/quickstarts/svelte): Get a SpacetimeDB Svelte app running in under 5 minutes.
- [TanStack Start Quickstart](/docs/quickstarts/tanstack): Get a SpacetimeDB app with TanStack Start running in under 5 minutes.
- [TypeScript Quickstart](/docs/quickstarts/typescript): Get a SpacetimeDB TypeScript app running in under 5 minutes.
- [Vue Quickstart](/docs/quickstarts/vue): Get a SpacetimeDB Vue app running in under 5 minutes.

### reference

- [SQL Reference](/docs/reference/sql): SpacetimeDB supports two subsets of SQL:

### resources

Guides, references, and tools to help you build with SpacetimeDB.

- [Developer Resources](/docs/resources): Guides, references, and tools to help you build with SpacetimeDB.

### sats-json

The Spacetime Algebraic Type System JSON format defines how Spacetime AlgebraicTypes and AlgebraicValues are encoded as JSON. Algebraic types and values are JSON-encoded for transport via the HTTP Databases API and the WebSocket text protocol. Note that SATS-JSON is not self-describing, and so a SATS value represented in JSON requires knowing the value's schema to meaningfully understand it - for example, it's not possible to tell whether a JSON object with a single field is a ProductValue with one element or a SumValue.

- [SATS-JSON Data Format](/docs/sats-json): The Spacetime Algebraic Type System JSON format defines how Spacetime AlgebraicTypes and AlgebraicValues are encoded as JSON. Algebraic types and values are JSON-encoded for transport via the HTTP Databases API and the WebSocket text protocol. Note that SATS-JSON is not self-describing, and so a SATS value represented in JSON requires knowing the value's schema to meaningfully understand it - for example, it's not possible to tell whether a JSON object with a single field is a ProductValue with one element or a SumValue.

### tables

Tables are the way to store data in SpacetimeDB. All data in SpacetimeDB is stored in memory for extremely low latency and high throughput access. SpacetimeDB also automatically persists all data to disk.

- [Tables](/docs/tables): Tables are the way to store data in SpacetimeDB. All data in SpacetimeDB is stored in memory for extremely low latency and high throughput access. SpacetimeDB also automatically persists all data to disk.
- [Table Access Permissions](/docs/tables/access-permissions): SpacetimeDB controls data access through table visibility and context-based permissions. Tables can be public or private, and different execution contexts (reducers, views, clients) have different levels of access.
- [Auto-Increment](/docs/tables/auto-increment): Auto-increment columns automatically generate unique integer values for new rows. When you insert a row with a zero value in an auto-increment column, SpacetimeDB assigns the next value from an internal sequence.
- [Column Types](/docs/tables/column-types): Columns define the structure of your tables. SpacetimeDB supports primitive types, composite types for complex data, and special types for database-specific functionality.
- [Constraints](/docs/tables/constraints): Constraints enforce data integrity rules on your tables. SpacetimeDB supports primary key and unique constraints.
- [Default Values](/docs/tables/default-values): Default values allow you to add new columns to existing tables during automatic migrations. When you republish a module with a new column that has a default value, existing rows are automatically populated with that default.
- [Event Tables](/docs/tables/event-tables): In many applications, particularly games and real-time systems, modules need to notify clients about things that happened without storing that information permanently. A combat system might need to tell clients "entity X took 50 damage" so they can display a floating damage number, but there is no reason to keep that record in the database after the moment has passed.
- [File Storage](/docs/tables/file-storage): SpacetimeDB can store binary data directly in table columns, making it suitable for files, images, and other blobs that need to participate in transactions and subscriptions.
- [Indexes](/docs/tables/indexes): Indexes accelerate queries by maintaining sorted data structures alongside your tables. Without an index, finding rows that match a condition requires scanning every row. With an index, the database locates matching rows directly.
- [Performance Best Practices](/docs/tables/performance): Follow these guidelines to optimize table performance in your SpacetimeDB modules.
- [Schedule Tables](/docs/tables/schedule-tables): Tables can trigger reducers or procedures at specific times by including a special scheduling column. This allows you to schedule future actions like sending reminders, expiring items, or running periodic maintenance tasks.

### tutorials

- [Chat App Tutorial](/docs/tutorials/chat-app): In this tutorial, we'll implement a simple chat server as a SpacetimeDB module. You can write your module in TypeScript, C#, or Rust - use the tabs throughout this guide to see code examples in your preferred language.
- [Unity Tutorial](/docs/tutorials/unity): Need help with the tutorial or CLI commands? Join our Discord server!
- [1 - Setup](/docs/tutorials/unity/part-1): Unity Tutorial Hero Image
- [2 - Connecting to SpacetimeDB](/docs/tutorials/unity/part-2): Need help with the tutorial? Join our Discord server!
- [3 - Gameplay](/docs/tutorials/unity/part-3): Need help with the tutorial? Join our Discord server!
- [4 - Moving and Colliding](/docs/tutorials/unity/part-4): Need help with the tutorial? Join our Discord server!
- [Unreal Tutorial](/docs/tutorials/unreal): Need help with the tutorial or CLI commands? Join our Discord server!
- [1 - Setup](/docs/tutorials/unreal/part-1): Need help with the tutorial? Join our Discord server!
- [2 - Connecting to SpacetimeDB](/docs/tutorials/unreal/part-2): Need help with the tutorial? Join our Discord server!
- [3 - Gameplay](/docs/tutorials/unreal/part-3): Need help with the tutorial? Join our Discord server!
- [4 - Moving and Colliding](/docs/tutorials/unreal/part-4): Need help with the tutorial? Join our Discord server!

### upgrade

This guide covers the breaking changes between SpacetimeDB 1.0 and 2.0 and how to update your code.

- [Migrating from SpacetimeDB 1.0 to 2.0](/docs/upgrade): This guide covers the breaking changes between SpacetimeDB 1.0 and 2.0 and how to update your code.

### webassembly-abi

This document specifies the low level details of module-host interactions ("Module ABI"). **Most users** looking to interact with the host will want to use derived and higher level functionality like bindings], #[spacetimedb(table)], and #[derive(SpacetimeType)] rather than this low level ABI. For more on those, read the [Rust module quick start guide and the Rust module reference.

- [Module ABI Reference](/docs/webassembly-abi): This document specifies the low level details of module-host interactions ("Module ABI"). **Most users** looking to interact with the host will want to use derived and higher level functionality like bindings], #[spacetimedb(table)], and #[derive(SpacetimeType)] rather than this low level ABI. For more on those, read the [Rust module quick start guide and the Rust module reference.
