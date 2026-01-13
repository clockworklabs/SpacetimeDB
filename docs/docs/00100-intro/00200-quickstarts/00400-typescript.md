---
title: TypeScript Quickstart
sidebar_label: TypeScript
slug: /quickstarts/typescript
hide_table_of_contents: true
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";
import { StepByStep, Step, StepText, StepCode } from "@site/src/components/Steps";


Get a SpacetimeDB TypeScript app running in under 5 minutes.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

<InstallCardLink />

---

<StepByStep>
  <Step title="Create your project">
    <StepText>
      Run the `spacetime dev` command to create a new project with a TypeScript SpacetimeDB module.

      This will start the local SpacetimeDB server, publish your module, and generate TypeScript client bindings.
    </StepText>
    <StepCode>
```bash
spacetime dev --template basic-typescript my-spacetime-app
```
    </StepCode>
  </Step>

  <Step title="Explore the project structure">
    <StepText>
      Your project contains both server and client code.

      Edit `spacetimedb/src/index.ts` to add tables and reducers. Use the generated bindings in `client/src/module_bindings/` to build your client.
    </StepText>
    <StepCode>
```
my-spacetime-app/
├── spacetimedb/          # Your SpacetimeDB module
│   └── src/
│       └── index.ts      # Server-side logic
├── client/               # Client application
│   └── src/
│       ├── index.ts
│       └── module_bindings/  # Auto-generated types
└── package.json
```
    </StepCode>
  </Step>

  <Step title="Test your module">
    <StepText>
      Use the CLI to interact with your running module. Call reducers and query data directly.
    </StepText>
    <StepCode>
```bash
# Call a reducer
spacetime call --server local my-spacetime-app your_reducer "arg1"

# Query your data
spacetime sql --server local my-spacetime-app "SELECT * FROM your_table"
```
    </StepCode>
  </Step>
</StepByStep>

## Next steps

- See the [Chat App Tutorial](/tutorials/chat-app) for a complete example
- Read the [TypeScript SDK Reference](/sdks/typescript) for detailed API docs
