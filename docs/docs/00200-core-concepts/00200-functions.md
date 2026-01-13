---
title: Functions
slug: /functions
---


Property / Characteristic | Reducers | Procedures | Views
-- | -- | -- | --
Read from tables | ✓ | ✓ | ✓
Write to tables | ✓ | ✓ |  
Runs in transaction by default | ✓ |   |  
Atomic | ✓ | (manual) | ✓
Deterministic | ✓ |   | ✓
External I/O (HTTP, etc.) |   | ✓ |  
Side-effecting |   | ✓ |  
Schedulable | ✓ | ✓ |  

SpacetimeDB modules can export three types of functions that clients can interact with:

## Reducers

**[Reducers](/functions/reducers)** are functions that modify database state in response to client requests or system events. They are the primary way to mutate tables in SpacetimeDB. Reducers run inside database transactions, providing isolation, atomicity, and consistency guarantees. If a reducer fails, all changes are automatically rolled back.

Reducers are isolated and cannot interact with the outside world - they can only perform database operations. Use reducers for all state-changing operations in your module.

## Procedures

**[Procedures](/functions/procedures)** are functions similar to reducers, but with the ability to perform operations beyond the database. Unlike reducers, procedures can make HTTP requests to external services. However, procedures don't automatically run in database transactions - they must manually open and commit transactions to read from or modify database state.

Procedures are currently in beta and should only be used when you need their special capabilities, such as making HTTP requests. For standard database operations, prefer using reducers.

## Views

**[Views](/functions/views)** are read-only functions that compute and return results from your tables. Unlike reducers and procedures, views do not modify database state - they only query and return data. Views are useful for computing derived data, aggregations, or joining multiple tables server-side before sending results to clients.

Views can be subscribed to just like tables and will automatically update clients when underlying data changes, making them ideal for real-time computed data.
