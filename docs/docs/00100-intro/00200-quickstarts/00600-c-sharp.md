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

  <Step title="Understand tables and reducers">
    <StepText>
      Open `spacetimedb/Lib.cs` to see the module code. The template includes a `Person` table and two reducers: `Add` to insert a person, and `SayHello` to greet everyone.

      Tables store your data. Reducers are functions that modify data — they're the only way to write to the database.
    </StepText>
    <StepCode>
```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Name = "Person", Public = true)]
    public partial struct Person
    {
        public string Name;
    }

    [SpacetimeDB.Reducer]
    public static void Add(ReducerContext ctx, string name)
    {
        ctx.Db.Person.Insert(new Person { Name = name });
    }

    [SpacetimeDB.Reducer]
    public static void SayHello(ReducerContext ctx)
    {
        foreach (var person in ctx.Db.Person.Iter())
        {
            Log.Info($"Hello, {person.Name}!");
        }
        Log.Info("Hello, World!");
    }
}
```
    </StepCode>
  </Step>

  <Step title="Test with the CLI">
    <StepText>
      Use the SpacetimeDB CLI to call reducers and query your data directly.
    </StepText>
    <StepCode>
```bash
# Call the add reducer to insert a person
spacetime call <database-name> Add Alice

# Query the person table
spacetime sql <database-name> "SELECT * FROM Person"
 name
---------
 "Alice"

# Call say_hello to greet everyone
spacetime call <database-name> SayHello

# View the module logs
spacetime logs <database-name>
2025-01-13T12:00:00.000000Z  INFO: Hello, Alice!
2025-01-13T12:00:00.000000Z  INFO: Hello, World!
```
    </StepCode>
  </Step>
</StepByStep>

## Next steps

- See the [Chat App Tutorial](/tutorials/chat-app) for a complete example
- Read the [C# SDK Reference](/sdks/c-sharp) for detailed API docs
