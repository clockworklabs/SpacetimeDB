---
title: Generating Client Bindings
slug: /sdks/codegen
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


Before you can interact with a SpacetimeDB [database](/databases) from a client application, you must generate client bindings for your **module**. These bindings create type-safe interfaces that allow your client to query [tables](/tables), invoke [reducers](/functions/reducers), call [procedures](/functions/procedures), and subscribe to [tables](/tables), and/or [views](/functions/views).

## What Are Module Bindings?

Module bindings are auto-generated code that mirrors your module's schema and functions. They provide:

- **Type definitions** matching your module's tables and types
- **Callable functions** for invoking reducers and procedures
- **Query interfaces** for subscriptions and local cache access
- **Callback registration** for observing database changes

The bindings ensure compile-time type safety between your client and server code, catching errors before runtime.

## Generating Bindings

Use the `spacetime generate` command to create bindings from your module:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```bash
mkdir -p src/module_bindings
spacetime generate --lang typescript --out-dir src/module_bindings --project-path PATH-TO-MODULE-DIRECTORY
```

This generates TypeScript files in `src/module_bindings/`. Import them in your client:

```typescript
import * as moduleBindings from './module_bindings';
```

Replace **PATH-TO-MODULE-DIRECTORY** with the path to your module's directory, where the module's `package.json` is located.

</TabItem>
<TabItem value="csharp" label="C#">

```bash
mkdir -p module_bindings
spacetime generate --lang cs --out-dir module_bindings --project-path PATH-TO-MODULE-DIRECTORY
```

This generates C# files in `module_bindings/`. The generated files are automatically included in your project.

Replace **PATH-TO-MODULE-DIRECTORY** with the path to your module's directory, where the module's `.csproj` is located.

</TabItem>
<TabItem value="rust" label="Rust">

```bash
mkdir -p src/module_bindings
spacetime generate --lang rust --out-dir client/src/module_bindings --project-path PATH-TO-MODULE-DIRECTORY
```

This generates Rust files in `client/src/module_bindings/`. Import them in your client with:

```rust
mod module_bindings;
```

Replace **PATH-TO-MODULE-DIRECTORY** with the path to your module's directory, where the module's `Cargo.toml` is located.

</TabItem>
<TabItem value="unreal" label="Unreal">

```bash
spacetime generate --lang unrealcpp --uproject-dir PATH-TO-UPROJECT --project-path PATH-TO-MODULE-DIRECTORY --module-name YOUR_MODULE_NAME
```

This generates Unreal C++ files in your project's `ModuleBindings` directory. The generated files are automatically included in your Unreal project.

Replace:
- **PATH-TO-UPROJECT** with the path to your Unreal project directory (containing the `.uproject` file)
- **PATH-TO-MODULE-DIRECTORY** with the path to your SpacetimeDB module
- **YOUR_MODULE_NAME** with the name of your Unreal module, typically the name of the project

</TabItem>
</Tabs>

## What Gets Generated

The `spacetime generate` command creates client-side representations of your module's components:

### Tables

For each [table](/tables) in your module, codegen generates:

- **Type/class definitions** with properties for each column
- **Table accessor** on the `DbConnection` for querying the client cache
- **Callback registration** methods for observing insertions, updates, and deletions

For example, a `user` table becomes:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Generated type
export default __t.object("User", {
  id: __t.u64(),
  name: __t.string(),
  email: __t.string(),
});

// Access via DbConnection
conn.db.User
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Generated type
public partial class User
{
    public ulong Id;
    public string Name;
    public string Email;
}

// Access via DbConnection
conn.Db.User
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Generated type
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
}

// Access via DbConnection
conn.db().user()
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
// Generated type
USTRUCT(BlueprintType)
struct FUser
{
    GENERATED_BODY()
    
    UPROPERTY(BlueprintReadWrite)
    int64 Id;
    
    UPROPERTY(BlueprintReadWrite)
    FString Name;
    
    UPROPERTY(BlueprintReadWrite)
    FString Email;
};

// Access via DbConnection
Context.Db->User
```

</TabItem>
</Tabs>

See the [Tables](/tables) documentation for details on defining tables in your module.

### Reducers

For each [reducer](/functions/reducers) in your module, codegen generates:

- **Client-callable function** that sends a reducer invocation request to the server
- **Callback registration** method for observing when the reducer runs
- **Type-safe parameters** matching the reducer's signature

For example, a `create_user` reducer becomes:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Call the reducer
conn.reducers.createUser(name, email);

// Register a callback to observe reducer invocations
conn.reducers.onCreateUser((ctx, name, email) => {
  console.log(`User created: ${name}`);
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Call the reducer
conn.Reducers.CreateUser(name, email);

// Register a callback to observe reducer invocations
conn.Reducers.OnCreateUser += (ctx, name, email) =>
{
    Console.WriteLine($"User created: {name}");
};
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Call the reducer
conn.reducers().create_user(name, email);

// Register a callback to observe reducer invocations
conn.reducers().on_create_user(|ctx, name, email| {
    println!("User created: {}", name);
});
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
// Call the reducer
Context.Reducers->CreateUser(TEXT("Alice"), TEXT("alice@example.com"));

// Register a callback to observe reducer invocations
FOnCreateUserDelegate Callback;
BIND_DELEGATE_SAFE(Callback, this, AMyActor, OnCreateUser);
Context.Reducers->OnCreateUser(Callback);

// Callback function (must be UFUNCTION)
UFUNCTION()
void OnCreateUser(const FReducerEventContext& Ctx, const FString& Name, const FString& Email)
{
    UE_LOG(LogTemp, Log, TEXT("User created: %s"), *Name);
}
```

</TabItem>
</Tabs>

See the [Reducers](/functions/reducers) documentation for details on defining reducers in your module.

### Procedures

For each [procedure](/functions/procedures) in your module, codegen generates:

- **Client-callable function** that invokes the procedure
- **Return value handling** for procedures that return results
- **Type-safe parameters** matching the procedure's signature

Procedures are currently in beta. For example, a `fetch_external_data` procedure becomes:

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Call the procedure
conn.procedures
  .fetchExternalData(url)
  .then(result => console.log(`Got result: ${result}`))
  .catch(error => console.error(`Error: ${error}`));
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
// Call the procedure without a callback
conn.Procedures.FetchExternalData(url);

// Call the procedure with a callback for the result
conn.Procedures.FetchExternalData(url, (ctx, result) =>
{
    if (result.IsSuccess)
    {
        Console.WriteLine($"Got result: {result.Value!}");
    }
    else
    {
        Console.WriteLine($"Error: {result.Error!}");
    }
});
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
// Call the procedure without a callback
conn.procedures().fetch_external_data(url);

// Call the procedure with a callback for the result
conn.procedures().fetch_external_data_then(url, |ctx, result| {
    match result {
        Ok(data) => println!("Got result: {:?}", data),
        Err(error) => eprintln!("Error: {:?}", error),
    }
});
```

</TabItem>
<TabItem value="unreal" label="Unreal">

```cpp
// Call the procedure without a callback
Context.Procedures->FetchExternalData(url, {});

// Call the procedure with a callback for the result
FOnFetchExternalDataComplete Callback;
BIND_DELEGATE_SAFE(Callback, this, AMyActor, OnFetchComplete);
Context.Procedures->FetchExternalData(url, Callback);

// Callback function (must be UFUNCTION)
UFUNCTION()
void OnFetchComplete(const FProcedureEventContext& Ctx, const FString& Result, bool bSuccess)
{
    if (bSuccess)
    {
        UE_LOG(LogTemp, Log, TEXT("Got result: %s"), *Result);
    }
    else
    {
        UE_LOG(LogTemp, Error, TEXT("Error"));
    }
}
```

</TabItem>
</Tabs>

See the [Procedures](/functions/procedures) documentation for details on defining procedures in your module.

### Views

For each [view](/functions/views) in your module, codegen generates:

- **Type definitions** for the view's return type
- **Subscription interfaces** for subscribing to view results
- **Query methods** for accessing cached view results

Views provide subscribable, computed queries over your data.

See the [Views](/functions/views) documentation for details on defining views in your module.

## Regenerating Bindings

Whenever you modify your module's schema or function signatures, regenerate the client bindings by running `spacetime generate` again. The tool will overwrite the existing generated files with updated code.

If you're actively developing and testing changes, consider adding `spacetime generate` to your build or development workflow.

## Using the Generated Code

Once you've generated the bindings, you're ready to connect to your database and start interacting with it. See:

- [Connecting to SpacetimeDB](/sdks/connection) for establishing a connection
- [SDK API Reference](/sdks/api) for using the generated bindings
- Language-specific references: [Rust](/sdks/rust), [C#](/sdks/c-sharp), [TypeScript](/sdks/typescript), [Unreal](/sdks/unreal)

## Troubleshooting

### Missing module directory

If `spacetime generate` fails to find your module, ensure you're pointing to the directory containing your module's project file (`Cargo.toml`, `.csproj`, or `package.json`).

### Outdated bindings

If your client doesn't see new tables or reducers, ensure you've regenerated the bindings after updating your module. Generated code is not automatically updated when the module changes.
