Get a SpacetimeDB Rust app running in under 5 minutes.

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

Install the [SpacetimeDB CLI](https://spacetimedb.com/install) before continuing.

---

## Create your project

Run the `spacetime dev` command to create a new project with a Rust SpacetimeDB module.

This will start the local SpacetimeDB server, compile and publish your module, and generate Rust client bindings.

```bash
spacetime dev --template basic-rs
```



## Explore the project structure

Your project contains both server and client code.

Edit `spacetimedb/src/lib.rs` to add tables and reducers. Use the generated bindings in `src/module_bindings/` to build your client.

```
my-spacetime-app/
├── spacetimedb/             # Your SpacetimeDB module
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs           # Server-side logic
├── Cargo.toml
├── src/
│   ├── main.rs              # Client application
│   └── module_bindings/     # Auto-generated types
└── README.md
```



## Understand tables and reducers

Open `spacetimedb/src/lib.rs` to see the module code. The template includes a `Person` table and two reducers: `add` to insert a person, and `say_hello` to greet everyone.

Tables store your data. Reducers are functions that modify data — they're the only way to write to the database.

```rust
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}
```



## Test with the CLI

Open a new terminal and navigate to your project directory. Then use the SpacetimeDB CLI to call reducers and query your data directly.

```bash
cd my-spacetime-app

# Call the add reducer to insert a person
spacetime call add Alice

# Query the person table
spacetime sql "SELECT * FROM person"
 name
---------
 "Alice"

# Call say_hello to greet everyone
spacetime call say_hello

# View the module logs
spacetime logs
2025-01-13T12:00:00.000000Z  INFO: Hello, Alice!
2025-01-13T12:00:00.000000Z  INFO: Hello, World!
```

## Next steps

- See the [Chat App Tutorial](https://spacetimedb.com/docs/intro/tutorials/chat-app) for a complete example
- Read the [Rust SDK Reference](https://spacetimedb.com/docs/intro/core-concepts/clients/rust-reference) for detailed API docs
