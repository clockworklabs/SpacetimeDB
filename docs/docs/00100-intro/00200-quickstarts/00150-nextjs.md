---
title: Next.js Quickstart
sidebar_label: Next.js
slug: /quickstarts/nextjs
hide_table_of_contents: true
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";
import { StepByStep, Step, StepText, StepCode } from "@site/src/components/Steps";


Get a SpacetimeDB Next.js app running in under 5 minutes.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

<InstallCardLink />

---

<StepByStep>
  <Step title="Create your project">
    <StepText>
      Run the `spacetime dev` command to create a new project with a SpacetimeDB module and Next.js client.

      This will start the local SpacetimeDB server, publish your module, generate TypeScript bindings, and start the Next.js development server.
    </StepText>
    <StepCode>
```bash
spacetime dev --template nextjs-ts my-nextjs-app
```
    </StepCode>
  </Step>

  <Step title="Open your app">
    <StepText>
      Navigate to [http://localhost:3001](http://localhost:3001) to see your app running.

      Note: The Next.js dev server runs on port 3001 to avoid conflict with SpacetimeDB on port 3000.
    </StepText>
  </Step>

  <Step title="Explore the project structure">
    <StepText>
      Your project contains both server and client code using the Next.js App Router.

      Edit `spacetimedb/src/index.ts` to add tables and reducers. Edit `app/page.tsx` to build your UI.
    </StepText>
    <StepCode>
```
my-nextjs-app/
├── spacetimedb/          # Your SpacetimeDB module
│   └── src/
│       └── index.ts      # Server-side logic
├── app/                  # Next.js App Router
│   ├── layout.tsx        # Root layout with providers
│   ├── page.tsx          # Home page
│   └── providers.tsx     # SpacetimeDB provider (client component)
├── src/
│   └── module_bindings/  # Auto-generated types
└── package.json
```
    </StepCode>
  </Step>

  <Step title="Understand the provider pattern">
    <StepText>
      SpacetimeDB requires a client-side connection. In Next.js App Router, this is handled by a client component wrapper.

      The `app/providers.tsx` file uses the `"use client"` directive and wraps your app with `SpacetimeDBProvider`.
    </StepText>
    <StepCode>
```tsx
// app/providers.tsx
'use client';

import { useMemo } from 'react';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection } from '../src/module_bindings';

export function Providers({ children }) {
  const connectionBuilder = useMemo(() =>
    DbConnection.builder()
      .withUri('ws://localhost:3000')
      .withModuleName('my-nextjs-app'),
    []
  );

  return (
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      {children}
    </SpacetimeDBProvider>
  );
}
```
    </StepCode>
  </Step>
</StepByStep>

## Next steps

- See the [Chat App Tutorial](/tutorials/chat-app) for a complete example
- Read the [TypeScript SDK Reference](/sdks/typescript) for detailed API docs
