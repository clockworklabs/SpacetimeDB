---
title: C# Quickstart
sidebar_label: C#
slug: /quickstarts/c-sharp
hide_table_of_contents: true
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";
import { StepByStep, Step, StepText, StepCode } from "@site/src/components/Steps";


Get a SpacetimeDB C# app running in under 5 minutes.

## Prerequisites

- [.NET 8 SDK](https://dotnet.microsoft.com/download/dotnet/8.0) installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

<InstallCardLink />

---

<StepByStep>
  <Step title="Install .NET WASI workload">
    <StepText>
      SpacetimeDB C# modules compile to WebAssembly using the WASI experimental workload.
    </StepText>
    <StepCode>
```bash
dotnet workload install wasi-experimental
```
    </StepCode>
  </Step>

  <Step title="Create your project">
    <StepText>
      Run the `spacetime dev` command to create a new project with a C# SpacetimeDB module.

      This will start the local SpacetimeDB server, compile and publish your module, and generate C# client bindings.
    </StepText>
    <StepCode>
```bash
spacetime dev --template basic-cs my-spacetime-app
```
    </StepCode>
  </Step>

  <Step title="Explore the project structure">
    <StepText>
      Your project contains both server and client code.

      Edit `spacetimedb/Lib.cs` to add tables and reducers. Use the generated bindings in the client project.
    </StepText>
    <StepCode>
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
    </StepCode>
  </Step>

  <Step title="Test your module">
    <StepText>
      Use the CLI to interact with your running module. Call reducers and query data directly.
    </StepText>
    <StepCode>
```bash
# Call a reducer
spacetime call --server local my-spacetime-app YourReducer "arg1"

# Query your data
spacetime sql --server local my-spacetime-app "SELECT * FROM your_table"
```
    </StepCode>
  </Step>
</StepByStep>

## Next steps

- See the [Chat App Tutorial](/tutorials/chat-app) for a complete example
- Read the [C# SDK Reference](/sdks/c-sharp) for detailed API docs
