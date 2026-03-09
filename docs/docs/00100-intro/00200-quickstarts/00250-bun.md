---
title: Bun Quickstart
sidebar_label: Bun
slug: /quickstarts/bun
hide_table_of_contents: true
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";
import { StepByStep, Step, StepText, StepCode } from "@site/src/components/Steps";

Get a SpacetimeDB Bun app running in under 5 minutes.

## Prerequisites

- [Bun](https://bun.sh/) installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

<InstallCardLink />

---

<StepByStep>
  <Step title="Create your project">
    <StepText>
      Run the `spacetime dev` command to create a new project with a SpacetimeDB module and Bun client.

      This will start the local SpacetimeDB server, publish your module, and generate TypeScript bindings.
    </StepText>
    <StepCode>

```bash
spacetime dev --template bun-ts
```

    </StepCode>

  </Step>

  <Step title="Explore the project structure">
    <StepText>
      Your project contains both server and client code.

      Edit `spacetimedb/src/index.ts` to add tables and reducers. Edit `src/main.ts` to build your Bun client.
    </StepText>
    <StepCode>

```
my-spacetime-app/
├── spacetimedb/          # Your SpacetimeDB module
│   └── src/
│       └── index.ts      # Server-side logic
├── src/
│   ├── main.ts           # Bun client script
│   └── module_bindings/  # Auto-generated types
└── package.json
```

    </StepCode>

  </Step>

  <Step title="Understand tables and reducers">
    <StepText>
      Open `spacetimedb/src/index.ts` to see the module code. The template includes a `person` table and two reducers: `add` to insert a person, and `sayHello` to greet everyone.

      Tables store your data. Reducers are functions that modify data — they're the only way to write to the database.
    </StepText>
    <StepCode>

```typescript
import { schema, table, t } from 'spacetimedb/server';

const spacetimedb = schema({
  person: table(
    { public: true },
    {
      name: t.string(),
    }
  ),
});
export default spacetimedb;

export const add = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    ctx.db.person.insert({ name });
  }
);

export const sayHello = spacetimedb.reducer(ctx => {
  for (const person of ctx.db.person.iter()) {
    console.info(`Hello, ${person.name}!`);
  }
  console.info('Hello, World!');
});
```

    </StepCode>

  </Step>

  <Step title="Run the client">
    <StepText>
      In a new terminal, run the Bun client. It will connect to SpacetimeDB and start an interactive CLI where you can add people and query the database.
    </StepText>
    <StepCode>
```bash
# Run with auto-reload during development
bun run dev

# Or run once

bun run start

```
    </StepCode>
  </Step>

  <Step title="Use the interactive CLI">
    <StepText>
      The client provides a command-line interface to interact with your SpacetimeDB module. Type a name to add a person, or use the built-in commands.
    </StepText>
    <StepCode>
```

Connecting to SpacetimeDB...
URI: ws://localhost:3000
Module: bun-ts

Connected to SpacetimeDB!
Identity: abc123def456...

Current people (0):
(none yet)

Commands:
<name> - Add a person with that name
list - Show all people
hello - Greet everyone (check server logs)
Ctrl+C - Quit

> Alice
> [Added] Alice

> Bob
> [Added] Bob

> list
> People in database:

- Alice
- Bob

> hello
> Called sayHello reducer (check server logs)

````
    </StepCode>
  </Step>

  <Step title="Understand the client code">
    <StepText>
      Open `src/main.ts` to see the Bun client. It uses `DbConnection.builder()` to connect to SpacetimeDB, subscribes to tables, and sets up the interactive CLI using Bun's native APIs.

      Unlike browser apps, Bun stores the authentication token in a file using `Bun.file()` and `Bun.write()`.
    </StepText>
    <StepCode>
```typescript
import { DbConnection } from './module_bindings/index.js';

// Build and establish connection
DbConnection.builder()
  .withUri(HOST)
  .withDatabaseName(DB_NAME)
  .withToken(await loadToken())  // Load saved token from file
  .onConnect((conn, identity, token) => {
    console.log('Connected! Identity:', identity.toHexString());
    saveToken(token);  // Save token for future connections

    // Subscribe to all tables
    conn.subscriptionBuilder()
      .onApplied((ctx) => {
        // Show current data, start CLI
        setupCLI(conn);
      })
      .subscribeToAllTables();

    // Listen for table changes
    conn.db.person.onInsert((ctx, person) => {
      console.log(`[Added] ${person.name}`);
    });
  })
  .build();
````

    </StepCode>

  </Step>

  <Step title="Test with the SpacetimeDB CLI">
    <StepText>
      You can also use the SpacetimeDB CLI to call reducers and query your data directly. Changes made via the CLI will appear in your Bun client in real-time.
    </StepText>
    <StepCode>
```bash
# Call the add reducer to insert a person
spacetime call add Charlie

# Query the person table

spacetime sql "SELECT \* FROM person"
name

---

"Alice"
"Bob"
"Charlie"

# Call sayHello to greet everyone

spacetime call say_hello

# View the module logs

spacetime logs
2025-01-13T12:00:00.000000Z INFO: Hello, Alice!
2025-01-13T12:00:00.000000Z INFO: Hello, Bob!
2025-01-13T12:00:00.000000Z INFO: Hello, Charlie!
2025-01-13T12:00:00.000000Z INFO: Hello, World!

````
    </StepCode>
  </Step>

  <Step title="Bun-specific features">
    <StepText>
      **Native WebSocket:** Bun has built-in WebSocket support, so no additional packages like `undici` are needed.

      **Built-in TypeScript:** Bun runs TypeScript directly without transpilation, making startup faster and eliminating the need for `tsx` or `ts-node`.

      **Environment variables:** Bun automatically loads `.env` files. Configure the connection using `SPACETIMEDB_HOST` and `SPACETIMEDB_DB_NAME` environment variables.

      **File APIs:** The template uses `Bun.file()` and `Bun.write()` for token persistence, which are faster than Node.js `fs` operations.
    </StepText>
    <StepCode>
```bash
# Configure via environment variables
SPACETIMEDB_HOST=ws://localhost:3000 \
SPACETIMEDB_DB_NAME=my-app \
bun run start

# Or create a .env file (Bun loads it automatically)
echo "SPACETIMEDB_HOST=ws://localhost:3000" > .env
echo "SPACETIMEDB_DB_NAME=my-app" >> .env
bun run start
````

    </StepCode>

  </Step>
</StepByStep>

## Next steps

- See the [Chat App Tutorial](../00300-tutorials/00100-chat-app.md) for a complete example
- Read the [TypeScript SDK Reference](../../00200-core-concepts/00600-clients/00700-typescript-reference.md) for detailed API docs
