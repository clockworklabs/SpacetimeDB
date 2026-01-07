---
title: C#
slug: /quickstarts/c-sharp
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";

# C# Quickstart

Get a SpacetimeDB C# app running in under 5 minutes.

## Prerequisites

- [.NET 8 SDK](https://dotnet.microsoft.com/download/dotnet/8.0) installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

<InstallCardLink />

## Install .NET WASI workload

SpacetimeDB C# modules require the WASI experimental workload:

```bash
dotnet workload install wasi-experimental
```

## Create your project

```bash
spacetime dev --template basic-c-sharp my-spacetime-app
```

This command:
1. Creates a new project with a C# SpacetimeDB module
2. Starts the local SpacetimeDB server
3. Compiles and publishes your module
4. Generates C# client bindings

## Project structure

```
my-spacetime-app/
├── spacetimedb/          # Your SpacetimeDB module
│   ├── StdbModule.csproj
│   └── Lib.cs            # Server-side logic
├── client/               # Client application
│   ├── Client.csproj
│   └── Program.cs
│       └── module_bindings/  # Auto-generated types
└── README.md
```

## Test your module

Call a reducer from the CLI:

```bash
spacetime call --server local my-spacetime-app YourReducer "arg1"
```

Query your data:

```bash
spacetime sql --server local my-spacetime-app "SELECT * FROM your_table"
```

## Next steps

- Edit `spacetimedb/Lib.cs` to add tables and reducers
- Build your client application using the generated bindings
- See the [Chat App Tutorial](/docs/tutorials/chat-app) for a complete example
- Read the [C# SDK Reference](/sdks/c-sharp) for detailed API docs
