Get a SpacetimeDB C# app running in under 5 minutes.

## Prerequisites

- [.NET 8 SDK](https://dotnet.microsoft.com/download/dotnet/8.0) installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

Install the [SpacetimeDB CLI](https://spacetimedb.com/install) before continuing.

---

## Install .NET WASI workload

SpacetimeDB C# modules compile to WebAssembly using the WASI experimental workload.

```bash
dotnet workload install wasi-experimental
```



## Create your project

Run the `spacetime dev` command to create a new project with a C# SpacetimeDB module.

This will start the local SpacetimeDB server, compile and publish your module, and generate C# client bindings.

```bash
spacetime dev --template basic-cs
```



## Explore the project structure

Your project contains both server and client code.

Edit `spacetimedb/Lib.cs` to add tables and reducers. Use the generated bindings in the client project.

```
my-spacetime-app/
├── spacetimedb/             # Your SpacetimeDB module
│   ├── StdbModule.csproj
│   └── Lib.cs               # Server-side logic
├── client.csproj
├── Program.cs               # Client application
└── module_bindings/         # Auto-generated types
```



## Understand tables and reducers

Open `spacetimedb/Lib.cs` to see the module code. The template includes a `Person` table and two reducers: `Add` to insert a person, and `SayHello` to greet everyone.

Tables store your data. Reducers are functions that modify data — they're the only way to write to the database.

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "Person", Public = true)]
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



## Test with the CLI

Open a new terminal and navigate to your project directory. Then use the SpacetimeDB CLI to call reducers and query your data directly.

```bash
cd my-spacetime-app

# Call the add reducer to insert a person
spacetime call add Alice

# Query the person table
spacetime sql "SELECT * FROM Person"
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
- Read the [C# SDK Reference](https://spacetimedb.com/docs/intro/core-concepts/clients/csharp-reference) for detailed API docs
