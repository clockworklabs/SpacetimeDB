---
title: TanStack Start Quickstart
sidebar_label: TanStack Start
slug: /quickstarts/tanstack
hide_table_of_contents: true
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";
import { StepByStep, Step, StepText, StepCode } from "@site/src/components/Steps";

Get a SpacetimeDB app with TanStack Start running in under 5 minutes.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

<InstallCardLink />

---

<StepByStep>
  <Step title="Create your project">
    <StepText>
      Run the `spacetime dev` command to create a new project with a SpacetimeDB module and TanStack Start.

      This will start the local SpacetimeDB server, publish your module, generate TypeScript bindings, and start the development server.
    </StepText>
    <StepCode>

```bash
spacetime dev --template tanstack-ts
```

    </StepCode>

  </Step>

  <Step title="Open your app">
    <StepText>
      Navigate to [http://localhost:5173](http://localhost:5173) to see your app running.

      The template includes a TanStack Start app with TanStack Query integration with SpacetimeDB.
    </StepText>

  </Step>

  <Step title="Explore the project structure">
    <StepText>
      Your project contains both server and client code.

      Edit `spacetimedb/src/index.ts` to add tables and reducers. Edit `src/routes/index.tsx` to build your UI.
    </StepText>
    <StepCode>

```
my-spacetime-app/
├── spacetimedb/          # Your SpacetimeDB module
│   └── src/
│       └── index.ts      # Server-side logic
├── src/                  # TanStack Start frontend
│   ├── router.tsx        # QueryClient + SpacetimeDB setup
│   ├── routes/
│   │   ├── __root.tsx    # Root layout
│   │   └── index.tsx     # Main app component
│   └── module_bindings/  # Auto-generated types
└── package.json
```

    </StepCode>

  </Step>

  <Step title="Understand tables and reducers">
    <StepText>
      Open `spacetimedb/src/index.ts` to see the module code. The template includes a `person` table and two reducers: `add` to insert a person, and `say_hello` to greet everyone.

      Tables store your data. Reducers are functions that modify data — they're the only way to write to the database.
    </StepText>
    <StepCode>
```typescript
import { schema, table, t } from 'spacetimedb/server';

export const spacetimedb = schema(
  table(
    { name: 'person', public: true },
    {
      name: t.string(),
    }
  )
);

spacetimedb.reducer('add', { name: t.string() }, (ctx, { name }) => {
  ctx.db.person.insert({ name });
});

spacetimedb.reducer('say_hello', (ctx) => {
  for (const person of ctx.db.person.iter()) {
    console.info(`Hello, ${person.name}!`);
  }
  console.info('Hello, World!');
});
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
    </StepCode>
  </Step>

  <Step title="Query and update data">
    <StepText>
      Use `useSpacetimeDBQuery()` to subscribe to tables with TanStack Query — it returns `[data, loading, query]`. SpacetimeDB React hooks also work with TanStack Start.
    </StepText>
    <StepCode>
```typescript
import { useSpacetimeDBQuery, useReducer } from 'spacetimedb/tanstack';
import { tables, reducers } from '../module_bindings';

function App() {
  const [people, loading] = useSpacetimeDBQuery(tables.person);
  const addPerson = useReducer(reducers.add);

  if (loading) return <p>Loading...</p>;

  return (
    <ul>
      {people.map((person, i) => (
        <li key={i}>{person.name}</li>
      ))}
    </ul>
  );
}
````

    </StepCode>

  </Step>
</StepByStep>

## Next steps

- See the [Chat App Tutorial](/tutorials/chat-app) for a complete example
- Read the [TypeScript SDK Reference](/clients/typescript) for detailed API docs
