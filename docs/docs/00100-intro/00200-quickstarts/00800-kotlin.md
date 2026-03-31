---
title: Kotlin Quickstart
sidebar_label: Kotlin
slug: /quickstarts/kotlin
hide_table_of_contents: true
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";
import { StepByStep, Step, StepText, StepCode } from "@site/src/components/Steps";


Get a SpacetimeDB Kotlin app running in under 5 minutes.

This quickstart uses the `basic-kt` template, a JVM-only console app. For a Kotlin Multiplatform project targeting Android and Desktop, use the `compose-kt` template instead.

## Prerequisites

- JDK 21+ installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

<InstallCardLink />

---

<StepByStep>
  <Step title="Create your project">
    <StepText>
      Run the `spacetime dev` command to create a new project with a Kotlin client and Rust server module.

      This will start the local SpacetimeDB server, compile and publish your module, and generate Kotlin client bindings.
    </StepText>
    <StepCode>
```bash
spacetime dev --template basic-kt
```
    </StepCode>
  </Step>

  <Step title="Explore the project structure">
    <StepText>
      Your project contains a Rust server module and a Kotlin client. The Gradle plugin auto-generates typed bindings into `build/generated/` on compile.
    </StepText>
    <StepCode>
```
my-spacetime-app/
├── spacetimedb/             # Your SpacetimeDB module (Rust)
│   ├── Cargo.toml
│   └── src/lib.rs           # Server-side logic
├── src/main/kotlin/
│   └── Main.kt              # Client application
├── build/generated/spacetimedb/
│   └── bindings/            # Auto-generated types
├── build.gradle.kts
└── settings.gradle.kts
```
    </StepCode>
  </Step>

  <Step title="Understand tables and reducers">
    <StepText>
      Open `spacetimedb/src/lib.rs` to see the module code. The template includes a `Person` table, three lifecycle reducers (`init`, `client_connected`, `client_disconnected`), and two application reducers: `add` to insert a person, and `say_hello` to greet everyone.

      Tables store your data. Reducers are functions that modify data — they're the only way to write to the database.
    </StepText>
    <StepCode>
```rust
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u64,
    name: String,
}

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {
    // Called when the module is initially published
}

#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(_ctx: &ReducerContext) {
    // Called everytime a new client connects
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(_ctx: &ReducerContext) {
    // Called everytime a client disconnects
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { id: 0, name });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}
```
    </StepCode>
  </Step>

  <Step title="See the Kotlin client">
    <StepText>
      Open `src/main/kotlin/Main.kt`. The client connects to SpacetimeDB, subscribes to tables, registers callbacks, and calls reducers — all with generated type-safe bindings.
    </StepText>
    <StepCode>
```kotlin
suspend fun main() {
    val host = System.getenv("SPACETIMEDB_HOST") ?: "ws://localhost:3000"
    val httpClient = HttpClient(OkHttp) { install(WebSockets) }

    DbConnection.Builder()
        .withHttpClient(httpClient)
        .withUri(host)
        .withDatabaseName(module_bindings.SpacetimeConfig.DATABASE_NAME)
        .withModuleBindings()
        .onConnect { conn, identity, _ ->
            println("Connected to SpacetimeDB!")
            println("Identity: ${identity.toHexString().take(16)}...")

            conn.db.person.onInsert { _, person ->
                println("New person: ${person.name}")
            }

            conn.reducers.onAdd { ctx, name ->
                println("[onAdd] Added person: $name (status=${ctx.status})")
            }

            conn.subscriptionBuilder()
                .onError { _, error -> println("Subscription error: $error") }
                .subscribeToAllTables()

            conn.reducers.add("Alice") { ctx ->
                println("[one-shot] Add completed: status=${ctx.status}")
                conn.reducers.sayHello()
            }
        }
        .onDisconnect { _, error ->
            if (error != null) {
                println("Disconnected with error: $error")
            } else {
                println("Disconnected")
            }
        }
        .onConnectError { _, error ->
            println("Connection error: $error")
        }
        .build()
        .use { delay(5.seconds) }
}
```
    </StepCode>
  </Step>

  <Step title="Test with the CLI">
    <StepText>
      Open a new terminal and navigate to your project directory. Then use the SpacetimeDB CLI to call reducers and query your data directly.
    </StepText>
    <StepCode>
```bash
cd my-spacetime-app

# Call the add reducer to insert a person
spacetime call add Alice

# Query the person table
spacetime sql "SELECT * FROM person"
 id | name
----+---------
  1 | "Alice"

# Call say_hello to greet everyone
spacetime call say_hello

# View the module logs
spacetime logs
2025-01-13T12:00:00.000000Z  INFO: Hello, Alice!
2025-01-13T12:00:00.000000Z  INFO: Hello, World!
```
    </StepCode>
  </Step>
</StepByStep>

## Next steps

- Read the [Kotlin SDK Reference](../../00200-core-concepts/00600-clients/00900-kotlin-reference.md) for detailed API docs
- Try the `compose-kt` template (`spacetime init --template compose-kt`) for a full KMP chat client with Compose Multiplatform
