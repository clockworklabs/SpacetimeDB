---
title: C# Reference
slug: /modules/c-sharp
toc_max_heading_level: 6
---

# C# Module Library

[SpacetimeDB](https://spacetimedb.com/) allows using the C# language to write server-side applications called **modules**. Modules, which run inside a relational database, have direct access to database tables, and expose public functions called **reducers** that can be invoked over the network. Clients connect directly to the database to read data.

```text
    Client Application                          SpacetimeDB
┌───────────────────────┐                ┌───────────────────────┐
│                       │                │                       │
│  ┌─────────────────┐  │    SQL Query   │  ┌─────────────────┐  │
│  │ Subscribed Data │<─────────────────────│    Database     │  │
│  └─────────────────┘  │                │  └─────────────────┘  │
│           │           │                │           ^           │
│           │           │                │           │           │
│           v           │                │           v           │
│  +─────────────────┐  │ call_reducer() │  ┌─────────────────┐  │
│  │   Client Code   │─────────────────────>│   Module Code   │  │
│  └─────────────────┘  │                │  └─────────────────┘  │
│                       │                │                       │
└───────────────────────┘                └───────────────────────┘
```

C# modules are written with the the C# Module Library (this package). They are built using the [dotnet CLI tool](https://learn.microsoft.com/en-us/dotnet/core/tools/) and deployed using the [`spacetime` CLI tool](https://spacetimedb.com/install). C# modules can import any [NuGet package](https://www.nuget.org/packages) that supports being compiled to WebAssembly.

(Note: C# can also be used to write **clients** of SpacetimeDB databases, but this requires using a different library, the SpacetimeDB C# Client SDK. See the documentation on [clients] for more information.)

This reference assumes you are familiar with the basics of C#. If you aren't, check out the [C# language documentation](https://learn.microsoft.com/en-us/dotnet/csharp/). For a guided introduction to C# Modules, see the [C# Module Quickstart](https://spacetimedb.com/docs/modules/c-sharp/quickstart).

## Overview

SpacetimeDB modules have two ways to interact with the outside world: tables and reducers.

- [Tables](#tables) store data and optionally make it readable by [clients].

- [Reducers](#reducers) are functions that modify data and can be invoked by [clients] over the network. They can read and write data in tables, and write to a private debug log.

These are the only ways for a SpacetimeDB module to interact with the outside world. Calling functions from `System.IO` or `System.Net` inside a reducer will result in runtime errors.

Declaring tables and reducers is straightforward:

```csharp
static partial class Module
{
    [SpacetimeDB.Table(Name = "player")]
    public partial struct Player
    {
        public int Id;
        public string Name;
    }

    [SpacetimeDB.Reducer]
    public static void AddPerson(ReducerContext ctx, int Id, string Name) {
        ctx.Db.player.Insert(new Player { Id = Id, Name = Name });
    }
}
```

Note that reducers don't return data directly; they can only modify the database. Clients connect directly to the database and use SQL to query [public](#public-and-private-tables) tables. Clients can also subscribe to a set of rows using SQL queries and receive streaming updates whenever any of those rows change.

Tables and reducers in C# modules can use any type annotated with [`[SpacetimeDB.Type]`](#attribute-spacetimedbtype).

<!-- TODO: link to client subscriptions / client one-off queries respectively. -->

## Setup

To create a C# module, install the [`spacetime` CLI tool](https://spacetimedb.com/install) in your preferred shell. Navigate to your work directory and run the following command:

```bash
spacetime init --lang csharp --project-path my-project-directory my-spacetimedb-project
```

This creates a `dotnet` project in `my-project-directory` with the following `StdbModule.csproj`:

```xml
<Project Sdk="Microsoft.NET.Sdk">

  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <RuntimeIdentifier>wasi-wasm</RuntimeIdentifier>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
  </PropertyGroup>

  <ItemGroup>
    <PackageReference Include="SpacetimeDB.Runtime" Version="1.0.0" />
  </ItemGroup>

</Project>
```

:::note

It is important to not change the `StdbModule.csproj` name because SpacetimeDB assumes that this will be the name of the file.

:::

This is a standard `csproj`, with the exception of the line `<RuntimeIdentifier>wasi-wasm</RuntimeIdentifier>`.
This line is important: it allows the project to be compiled to a WebAssembly module.

The project's `Lib.cs` will contain the following skeleton:

```csharp
public static partial class Module
{
    [SpacetimeDB.Table]
    public partial struct Person
    {
        [SpacetimeDB.AutoInc]
        [SpacetimeDB.PrimaryKey]
        public int Id;
        public string Name;
        public int Age;
    }

    [SpacetimeDB.Reducer]
    public static void Add(ReducerContext ctx, string name, int age)
    {
        var person = ctx.Db.Person.Insert(new Person { Name = name, Age = age });
        Log.Info($"Inserted {person.Name} under #{person.Id}");
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

This skeleton declares a [table](#tables) and some [reducers](#reducers).

You can also add some [lifecycle reducers](#lifecycle-reducers) to the `Module` class using the following code:

```csharp
[Reducer(ReducerKind.Init)]
public static void Init(ReducerContext ctx)
{
    // Run when the module is first loaded.
}

[Reducer(ReducerKind.ClientConnected)]
public static void ClientConnected(ReducerContext ctx)
{
    // Called when a client connects.
}

[Reducer(ReducerKind.ClientDisconnected)]
public static void ClientDisconnected(ReducerContext ctx)
{
    // Called when a client connects.
}
```

To compile the project, run the following command:

```bash
spacetime build
```

SpacetimeDB requires a WebAssembly-compatible `dotnet` toolchain. If the `spacetime` cli finds a compatible version of [`dotnet`](https://rustup.rs/) that it can run, it will automatically install the `wasi-experimental` workload and use it to build your application. This can also be done manually using the command:

```bash
dotnet workload install wasi-experimental
```

If you are managing your dotnet installation in some other way, you will need to install the `wasi-experimental` workload yourself.

To build your application and upload it to the public SpacetimeDB network, run:

```bash
spacetime login
```

And then:

```bash
spacetime publish [MY_DATABASE_NAME]
```

For example:

```bash
spacetime publish silly_demo_app
```

When you publish your module, a database named `silly_demo_app` will be created with the requested tables, and the module will be installed inside it.

The output of `spacetime publish` will end with a line:

```text
Created new database with name: <name>, identity: <hex string>
```

This name is the human-readable name of the created database, and the hex string is its [`Identity`](#struct-identity). These distinguish the created database from the other databases running on the SpacetimeDB network. They are used when administering the application, for example using the [`spacetime logs <DATABASE_NAME>`](#class-log) command. You should probably write the database name down in a text file so that you can remember it.

After modifying your project, you can run:

`spacetime publish <DATABASE_NAME>`

to update the module attached to your database. Note that SpacetimeDB tries to [automatically migrate](#automatic-migrations) your database schema whenever you run `spacetime publish`.

You can also generate code for clients of your module using the `spacetime generate` command. See the [client SDK documentation] for more information.

## How it works

Under the hood, SpacetimeDB modules are WebAssembly modules that import a [specific WebAssembly ABI](https://spacetimedb.com/docs/webassembly-abi) and export a small number of special functions. This is automatically configured when you add the `SpacetimeDB.Runtime` package as a dependency of your application.

The SpacetimeDB host is an application that hosts SpacetimeDB databases. [Its source code is available](https://github.com/clockworklabs/SpacetimeDB) under [the Business Source License with an Additional Use Grant](https://github.com/clockworklabs/SpacetimeDB/blob/master/LICENSE.txt). You can run your own host, or you can upload your module to the public SpacetimeDB network. <!-- TODO: want a link to some dashboard for the public network. --> The network will create a database for you and install your module in it to serve client requests.

### In More Detail: Publishing a Module

The `spacetime publish [DATABASE_IDENTITY]` command compiles a module and uploads it to a SpacetimeDB host. After this:

- The host finds the database with the requested `DATABASE_IDENTITY`.
  - (Or creates a fresh database and identity, if no identity was provided).
- The host loads the new module and inspects its requested database schema. If there are changes to the schema, the host tries perform an [automatic migration](#automatic-migrations). If the migration fails, publishing fails.
- The host terminates the old module attached to the database.
- The host installs the new module into the database. It begins running the module's [lifecycle reducers](#lifecycle-reducers) and [scheduled reducers](#scheduled-reducers), starting with the `Init` reducer.
- The host begins allowing clients to call the module's reducers.

From the perspective of clients, this process is seamless. Open connections are maintained and subscriptions continue functioning. [Automatic migrations](#automatic-migrations) forbid most table changes except for adding new tables, so client code does not need to be recompiled.
However:

- Clients may witness a brief interruption in the execution of scheduled reducers (for example, game loops.)
- New versions of a module may remove or change reducers that were previously present. Client code calling those reducers will receive runtime errors.

## Tables

Tables are declared using the `[SpacetimeDB.Table]` attribute.

This macro is applied to a C# `partial class` or `partial struct` with named fields. (The `partial` modifier is required to allow code generation to add methods.) All of the fields of the table must be marked with [`[SpacetimeDB.Type]`](#attribute-spacetimedbtype).

The resulting type is used to store rows of the table. It's a normal class (or struct). Row values are not special -- operations on row types do not, by themselves, modify the table. Instead, a [`ReducerContext`](#class-reducercontext) is needed to get a handle to the table.

```csharp
public static partial class Module {

    /// <summary>
    /// A Person is a row of the table person.
    /// </summary>
    [SpacetimeDB.Table(Name = "person", Public)]
    public partial struct Person {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        ulong Id;
        [SpacetimeDB.Index.BTree]
        string Name;
    }

    // `Person` is a normal C# struct type.
    // Operations on a `Person` do not, by themselves, do anything.
    // The following function does not interact with the database at all.
    public static void DoNothing() {
        // Creating a `Person` DOES NOT modify the database.
        var person = new Person { Id = 0, Name = "Joe Average" };
        // Updating a `Person` DOES NOT modify the database.
        person.Name = "Joanna Average";
        // Deallocating a `Person` DOES NOT modify the database.
        person = null;
    }

    // To interact with the database, you need a `ReducerContext`,
    // which is provided as the first parameter of any reducer.
    [SpacetimeDB.Reducer]
    public static void DoSomething(ReducerContext ctx) {
        // The following inserts a row into the table:
        var examplePerson = ctx.Db.person.Insert(new Person { id = 0, name = "Joe Average" });

        // `examplePerson` is a COPY of the row stored in the database.
        // If we update it:
        examplePerson.name = "Joanna Average".to_string();
        // Our copy is now updated, but the database's copy is UNCHANGED.
        // To push our change through, we can call `UniqueIndex.Update()`:
        examplePerson = ctx.Db.person.Id.Update(examplePerson);
        // Now the database and our copy are in sync again.

        // We can also delete the row in the database using `UniqueIndex.Delete()`.
        ctx.Db.person.Id.Delete(examplePerson.Id);
    }
}
```

(See [reducers](#reducers) for more information on declaring reducers.)

This library generates a custom API for each table, depending on the table's name and structure.

All tables support getting a handle implementing the [`ITableView`](#interface-itableview) interface from a [`ReducerContext`](#class-reducercontext), using:

```text
ctx.Db.{table_name}
```

For example,

```csharp
ctx.Db.person
```

[Unique and primary key columns](#unique-and-primary-key-columns) and [indexes](#indexes) generate additional accessors, such as `ctx.Db.person.Id` and `ctx.Db.person.Name`.

### Interface `ITableView`

```csharp
namespace SpacetimeDB.Internal;

public interface ITableView<View, Row>
    where Row : IStructuralReadWrite, new()
{
        /* ... */
}
```

<!-- Actually, `Row` is called `T` in the real declaration, but it would be much clearer
if it was called `Row`. -->

Implemented for every table handle generated by the [`Table`](#tables) attribute.
For a table named `{name}`, a handle can be extracted from a [`ReducerContext`](#class-reducercontext) using `ctx.Db.{name}`. For example, `ctx.Db.person`.

Contains methods that are present for every table handle, regardless of what unique constraints
and indexes are present.

The type `Row` is the type of rows in the table.

| Name                                          | Description                   |
| --------------------------------------------- | ----------------------------- |
| [Method `Insert`](#method-itableviewinsert)   | Insert a row into the table   |
| [Method `Delete`](#method-itableviewdelete)   | Delete a row from the table   |
| [Method `Iter`](#method-itableviewiter)       | Iterate all rows of the table |
| [Property `Count`](#property-itableviewcount) | Count all rows of the table   |

#### Method `ITableView.Insert`

```csharp
Row Insert(Row row);
```

Inserts `row` into the table.

The return value is the inserted row, with any auto-incrementing columns replaced with computed values.
The `insert` method always returns the inserted row, even when the table contains no auto-incrementing columns.

(The returned row is a copy of the row in the database.
Modifying this copy does not directly modify the database.
See [`UniqueIndex.Update()`](#method-uniqueindexupdate) if you want to update the row.)

Throws an exception if inserting the row violates any constraints.

Inserting a duplicate row in a table is a no-op,
as SpacetimeDB is a set-semantic database.

#### Method `ITableView.Delete`

```csharp
bool Delete(Row row);
```

Deletes a row equal to `row` from the table.

Returns `true` if the row was present and has been deleted,
or `false` if the row was not present and therefore the tables have not changed.

Unlike [`Insert`](#method-itableviewinsert), there is no need to return the deleted row,
as it must necessarily have been exactly equal to the `row` argument.
No analogue to auto-increment placeholders exists for deletions.

Throws an exception if deleting the row would violate any constraints.

#### Method `ITableView.Iter`

```csharp
IEnumerable<Row> Iter();
```

Iterate over all rows of the table.

(This keeps track of changes made to the table since the start of this reducer invocation. For example, if rows have been [deleted](#method-itableviewdelete) since the start of this reducer invocation, those rows will not be returned by `Iter`. Similarly, [inserted](#method-itableviewinsert) rows WILL be returned.)

For large tables, this can be a slow operation! Prefer [filtering](#method-indexfilter) by an [`Index`](#class-index) or [finding](#method-uniqueindexfind) a [`UniqueIndex`](#class-uniqueindex) if possible.

#### Property `ITableView.Count`

```csharp
ulong Count { get; }
```

Returns the number of rows of this table.

This takes into account modifications by the current transaction,
even though those modifications have not yet been committed or broadcast to clients.
This applies generally to insertions, deletions, updates, and iteration as well.

### Public and Private Tables

By default, tables are considered **private**. This means that they are only readable by the database owner and by reducers. Reducers run inside the database, so clients cannot see private tables at all or even know of their existence.

Using the `[SpacetimeDB.Table(Name = "table_name", Public)]` flag makes a table public. **Public** tables are readable by all clients. They can still only be modified by reducers.

(Note that, when run by the module owner, the `spacetime sql <SQL_QUERY>` command can also read private tables. This is for debugging convenience. Only the module owner can see these tables. This is determined by the `Identity` stored by the `spacetime login` command. Run `spacetime login show` to print your current logged-in `Identity`.)

To learn how to subscribe to a public table, see the [client SDK documentation](https://spacetimedb.com/docs/sdks).

### Unique and Primary Key Columns

Columns of a table (that is, fields of a [`[Table]`](#tables) struct) can be annotated with `[Unique]` or `[PrimaryKey]`. Multiple columns can be `[Unique]`, but only one can be `[PrimaryKey]`. For example:

```csharp
[SpacetimeDB.Table(Name = "citizen")]
public partial struct Citizen {
    [SpacetimeDB.PrimaryKey]
    ulong Id;

    [SpacetimeDB.Unique]
    string Ssn;

    [SpacetimeDB.Unique]
    string Email;

    string name;
}
```

Every row in the table `Person` must have unique entries in the `id`, `ssn`, and `email` columns. Attempting to insert multiple `Person`s with the same `id`, `ssn`, or `email` will throw an exception.

Any `[Unique]` or `[PrimaryKey]` column supports getting a [`UniqueIndex`](#class-uniqueindex) from a [`ReducerContext`](#class-reducercontext) using:

```text
ctx.Db.{table}.{unique_column}
```

For example,

```csharp
ctx.Db.citizen.Ssn
```

Notice that updating a row is only possible if a row has a unique column -- there is no `update` method in the base [`ITableView`](#interface-itableview) interface. SpacetimeDB has no notion of rows having an "identity" aside from their unique / primary keys.

The `[PrimaryKey]` annotation implies a `[Unique]` annotation, but avails additional methods in the [client]-side SDKs.

It is not currently possible to mark a group of fields as collectively unique.

Filtering on unique columns is only supported for a limited number of types.

### Class `UniqueIndex`

```csharp
namespace SpacetimeDB.Internal;

public abstract class UniqueIndex<Handle, Row, Column, RW> : IndexBase<Row>
    where Handle : ITableView<Handle, Row>
    where Row : IStructuralReadWrite, new()
    where Column : IEquatable<Column>
{
    /* ... */
}
```

<!-- Actually, `Column` is called `T` in the real declaration, but it would be much clearer
if it was called `Column`. -->

A unique index on a column. Available for `[Unique]` and `[PrimaryKey]` columns.
(A custom class derived from `UniqueIndex` is generated for every such column.)

`Row` is the type decorated with `[SpacetimeDB.Table]`, `Column` is the type of the column,
and `Handle` is the type of the generated table handle.

For a table _table_ with a column _column_, use `ctx.Db.{table}.{column}`
to get a `UniqueColumn` from a [`ReducerContext`](#class-reducercontext).

Example:

```csharp
using SpacetimeDB;

public static partial class Module {
    [Table(Name = "user")]
    public partial struct User {
        [PrimaryKey]
        uint Id;
        [Unique]
        string Username;
        ulong DogCount;
    }

    [Reducer]
    void Demo(ReducerContext ctx) {
        var idIndex = ctx.Db.user.Id;
        var exampleUser = idIndex.Find(357).Value;
        exampleUser.DogCount += 5;
        idIndex.Update(exampleUser);

        var usernameIndex = ctx.Db.user.Username;
        usernameIndex.Delete("Evil Bob");
    }
}
```

| Name                                         | Description                                  |
| -------------------------------------------- | -------------------------------------------- |
| [Method `Find`](#method-uniqueindexfind)     | Find a row by the value of a unique column   |
| [Method `Update`](#method-uniqueindexupdate) | Update a row with a unique column            |
| [Method `Delete`](#method-uniqueindexdelete) | Delete a row by the value of a unique column |

<!-- Technically, these methods only exist in the generated code, not in the abstract
base class. This is a wart that is necessary because of a bad interaction between C# inheritance, nullable types, and structs/classes.-->

#### Method `UniqueIndex.Find`

```csharp
Row? Find(Column key);
```

Finds and returns the row where the value in the unique column matches the supplied `key`,
or `null` if no such row is present in the database state.

#### Method `UniqueIndex.Update`

```csharp
Row Update(Row row);
```

Deletes the row where the value in the unique column matches that in the corresponding field of `row` and then inserts `row`.

Returns the new row as actually inserted, with any auto-inc placeholders substituted for computed values.

Throws if no row was previously present with the matching value in the unique column,
or if either the delete or the insertion would violate a constraint.

#### Method `UniqueIndex.Delete`

```csharp
bool Delete(Column key);
```

Deletes the row where the value in the unique column matches the supplied `key`, if any such row is present in the database state.

Returns `true` if a row with the specified `key` was previously present and has been deleted,
or `false` if no such row was present.

### Auto-inc columns

Columns can be marked `[SpacetimeDB.AutoInc]`. This can only be used on integer types (`int`, `ulong`, etc.)

When inserting into or updating a row in a table with an `[AutoInc]` column, if the annotated column is set to zero (`0`), the database will automatically overwrite that zero with an atomically increasing value.

[`ITableView.Insert`] and [`UniqueIndex.Update()`](#method-uniqueindexupdate) returns rows with `[AutoInc]` columns set to the values that were actually written into the database.

```csharp
public static partial class Module
{
    [SpacetimeDB.Table(Name = "example")]
    public partial struct Example
    {
        [SpacetimeDB.AutoInc]
        public int Field;
    }

    [SpacetimeDB.Reducer]
    public static void InsertAutoIncExample(ReducerContext ctx, int Id, string Name) {
        for (var i = 0; i < 10; i++) {
            // These will have distinct, unique values
            // at rest in the database, since they
            // are inserted with the sentinel value 0.
            var actual = ctx.Db.example.Insert(new Example { Field = 0 });
            Debug.Assert(actual.Field != 0);
        }
    }
}
```

`[AutoInc]` is often combined with `[Unique]` or `[PrimaryKey]` to automatically assign unique integer identifiers to rows.

### Indexes

SpacetimeDB supports both single- and multi-column [B-Tree](https://en.wikipedia.org/wiki/B-tree) indexes.

Indexes are declared using the syntax:

```csharp
[SpacetimeDB.Index.BTree(Name = "IndexName", Columns = [nameof(Column1), nameof(Column2), nameof(Column3)])]
```

For example:

```csharp
[SpacetimeDB.Table(Name = "paper")]
[SpacetimeDB.Index.BTree(Name = "TitleAndDate", Columns = [nameof(Title), nameof(Date)])]
[SpacetimeDB.Index.BTree(Name = "UrlAndCountry", Columns = [nameof(Url), nameof(Country)])]
public partial struct AcademicPaper {
    public string Title;
    public string Url;
    public string Date;
    public string Venue;
    public string Country;
}
```

Multiple indexes can be declared.

Single-column indexes can also be declared using an annotation on a column:

```csharp
[SpacetimeDB.Table(Name = "academic_paper")]
public partial struct AcademicPaper {
    public string Title;
    public string Url;
    [SpacetimeDB.Index.BTree] // The index will be named "Date".
    public string Date;
    [SpacetimeDB.Index.BTree] // The index will be named "Venue".
    public string Venue;
    [SpacetimeDB.Index.BTree(Name = "ByCountry")] // The index will be named "ByCountry".
    public string Country;
}
```

Any table supports getting an [`Index`](#class-index) using `ctx.Db.{table}.{index}`. For example, `ctx.Db.academic_paper.TitleAndDate` or `ctx.Db.academic_paper.Venue`.

### Indexable Types

SpacetimeDB supports only a restricted set of types as index keys:

- Signed and unsigned integers of various widths.
- `bool`.
- `string`.
- [`Identity`](#struct-identity).
- [`ConnectionId`](#struct-connectionid).
- `enum`s annotated with [`SpacetimeDB.Type`](#attribute-spacetimedbtype).

### Class `Index`

```csharp
public abstract class IndexBase<Row>
    where Row : IStructuralReadWrite, new()
{
    // ...
}
```

Each index generates a subclass of `IndexBase`, which is accessible via `ctx.Db.{table}.{index}`. For example, `ctx.Db.academic_paper.TitleAndDate`.

Indexes can be applied to a variable number of columns, referred to as `Column1`, `Column2`, `Column3`... in the following examples.

| Name                                   | Description             |
| -------------------------------------- | ----------------------- |
| Method [`Filter`](#method-indexfilter) | Filter rows in an index |
| Method [`Delete`](#method-indexdelete) | Delete rows in an index |

#### Method `Index.Filter`

```csharp
public IEnumerable<Row> Filter(Column1 bound);
public IEnumerable<Row> Filter(Bound<Column1> bound);
public IEnumerable<Row> Filter((Column1, Column2) bound);
public IEnumerable<Row> Filter((Column1, Bound<Column2>) bound);
public IEnumerable<Row> Filter((Column1, Column2, Column3) bound);
public IEnumerable<Row> Filter((Column1, Column2, Bound<Column3>) bound);
// ...
```

Returns an iterator over all rows in the database state where the indexed column(s) match the passed `bound`. Bound is a tuple of column values, possibly terminated by a `Bound<LastColumn>`. A `Bound<LastColumn>` is simply a tuple `(LastColumn Min, LastColumn Max)`. Any prefix of the indexed columns can be passed, for example:

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Name = "zoo_animal")]
    [SpacetimeDB.Index.BTree(Name = "SpeciesAgeName", Columns = [nameof(Species), nameof(Age), nameof(Name)])]
    public partial struct ZooAnimal
    {
        public string Species;
        public uint Age;
        public string Name;
        [SpacetimeDB.PrimaryKey]
        public uint Id;
    }

    [SpacetimeDB.Reducer]
    public static void Example(ReducerContext ctx)
    {
        foreach (var baboon in ctx.Db.zoo_animal.SpeciesAgeName.Filter("baboon"))
        {
            // Work with the baboon.
        }
        foreach (var animal in ctx.Db.zoo_animal.SpeciesAgeName.Filter(("b", "e")))
        {
            // Work with the animal.
            // The name of the species starts with a character between "b" and "e".
        }
        foreach (var babyBaboon in ctx.Db.zoo_animal.SpeciesAgeName.Filter(("baboon", 1)))
        {
            // Work with the baby baboon.
        }
        foreach (var youngBaboon in ctx.Db.zoo_animal.SpeciesAgeName.Filter(("baboon", (1, 5))))
        {
            // Work with the young baboon.
        }
        foreach (var babyBaboonNamedBob in ctx.Db.zoo_animal.SpeciesAgeName.Filter(("baboon", 1, "Bob")))
        {
            // Work with the baby baboon named "Bob".
        }
        foreach (var babyBaboon in ctx.Db.zoo_animal.SpeciesAgeName.Filter(("baboon", 1, ("a", "f"))))
        {
            // Work with the baby baboon, whose name starts with a letter between "a" and "f".
        }
    }
}
```

#### Method `Index.Delete`

```csharp
public ulong Delete(Column1 bound);
public ulong Delete(Bound<Column1> bound);
public ulong Delete((Column1, Column2) bound);
public ulong Delete((Column1, Bound<Column2>) bound);
public ulong Delete((Column1, Column2, Column3) bound);
public ulong Delete((Column1, Column2, Bound<Column3>) bound);
// ...
```

Delete all rows in the database state where the indexed column(s) match the passed `bound`. Returns the count of rows deleted. Note that there may be multiple rows deleted even if only a single column value is passed, since the index is not guaranteed to be unique.

## Reducers

Reducers are declared using the `[SpacetimeDB.Reducer]` attribute.

`[SpacetimeDB.Reducer]` is always applied to static C# functions. The first parameter of a reducer must be a [`ReducerContext`]. The remaining parameters must be types marked with [`SpacetimeDB.Type`]. Reducers should return `void`.

```csharp
public static partial class Module {
    [SpacetimeDB.Reducer]
    public static void GivePlayerItem(
        ReducerContext context,
        ulong PlayerId,
        ulong ItemId
    )
    {
        // ...
    }
}
```

Every reducer runs inside a [database transaction](https://en.wikipedia.org/wiki/Database_transaction). <!-- TODO: specific transaction level guarantees. --> This means that reducers will not observe the effects of other reducers modifying the database while they run. If a reducer fails, all of its changes to the database will automatically be rolled back. Reducers can fail by throwing an exception.

### Class `ReducerContext`

```csharp
public sealed record ReducerContext : DbContext<Local>, Internal.IReducerContext
{
    // ...
}
```

Reducers have access to a special [`ReducerContext`] parameter. This parameter allows reading and writing the database attached to a module. It also provides some additional functionality, like generating random numbers and scheduling future operations.

[`ReducerContext`] provides access to the database tables via [the `.Db` property](#property-reducercontextdb). The [`[Table]`](#tables) attribute generated code that adds table accessors to this property.

| Name                                                            | Description                                                                     |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| Property [`Db`](#property-reducercontextdb)                     | The current state of the database                                               |
| Property [`Sender`](#property-reducercontextsender)             | The [`Identity`](#struct-identity) of the caller of the reducer                 |
| Property [`ConnectionId`](#property-reducercontextconnectionid) | The [`ConnectionId`](#struct-connectionid) of the caller of the reducer, if any |
| Property [`Rng`](#property-reducercontextrng)                   | A [`System.Random`] instance.                                                   |
| Property [`Timestamp`](#property-reducercontexttimestamp)       | The [`Timestamp`](#struct-timestamp) of the reducer invocation                  |
| Property [`Identity`](#property-reducercontextidentity)         | The [`Identity`](#struct-identity) of the module                                |

#### Property `ReducerContext.Db`

```csharp
DbView Db;
```

Allows accessing the local database attached to a module.

The `[Table]` attribute generates a field of this property.

For a table named _table_, use `ctx.Db.{table}` to get a [table view](#interface-itableview).
For example, `ctx.Db.users`.

You can also use `ctx.Db.{table}.{index}` to get an [index](#class-index) or [unique index](#class-uniqueindex).

#### Property `ReducerContext.Sender`

```csharp
Identity Sender;
```

The [`Identity`](#struct-identity) of the client that invoked the reducer.

#### Property `ReducerContext.ConnectionId`

```csharp
ConnectionId? ConnectionId;
```

The [`ConnectionId`](#struct-connectionid) of the client that invoked the reducer.

`null` if no `ConnectionId` was supplied to the `/database/call` HTTP endpoint,
or via the CLI's `spacetime call` subcommand.

#### Property `ReducerContext.Rng`

```csharp
Random Rng;
```

A [`System.Random`] that can be used to generate random numbers.

#### Property `ReducerContext.Timestamp`

```csharp
Timestamp Timestamp;
```

The time at which the reducer was invoked.

#### Property `ReducerContext.Identity`

```csharp
Identity Identity;
```

The [`Identity`](#struct-identity) of the module.

This can be used to [check whether a scheduled reducer is being called by a user](#restricting-scheduled-reducers).

Note: this is not the identity of the caller, that's [`ReducerContext.Sender`](#property-reducercontextsender).

### Lifecycle Reducers

A small group of reducers are called at set points in the module lifecycle. These are used to initialize
the database and respond to client connections. You can have one of each per module.

These reducers cannot be called manually and may not have any parameters except for `ReducerContext`.

#### The `Init` reducer

This reducer is marked with `[SpacetimeDB.Reducer(ReducerKind.Init)]`. It is run the first time a module is published and any time the database is cleared.

If an error occurs when initializing, the module will not be published.

This reducer can be used to configure any static data tables used by your module. It can also be used to start running [scheduled reducers](#scheduled-reducers).

#### The `ClientConnected` reducer

This reducer is marked with `[SpacetimeDB.Reducer(ReducerKind.ClientConnected)]`. It is run when a client connects to the SpacetimeDB database. Their identity can be found in the sender value of the `ReducerContext`.

If an error occurs in the reducer, the client will be disconnected.

#### The `ClientDisconnected` reducer

This reducer is marked with `[SpacetimeDB.Reducer(ReducerKind.ClientDisconnected)]`. It is run when a client disconnects from the SpacetimeDB database. Their identity can be found in the sender value of the `ReducerContext`.

If an error occurs in the disconnect reducer, the client is still recorded as disconnected.

### Scheduled Reducers

Reducers can schedule other reducers to run asynchronously. This allows calling the reducers at a particular time, or at repeating intervals. This can be used to implement timers, game loops, and maintenance tasks.

The scheduling information for a reducer is stored in a table.
This table has two mandatory fields:

- An `[AutoInc] [PrimaryKey] ulong` field that identifies scheduled reducer calls.
- A [`ScheduleAt`](#record-scheduleat) field that says when to call the reducer.

Managing timers with a scheduled table is as simple as inserting or deleting rows from the table.
This makes scheduling transactional in SpacetimeDB. If a reducer A first schedules B but then errors for some other reason, B will not be scheduled to run.

A [`ScheduleAt`](#record-scheduleat) can be created from a [`Timestamp`](#struct-timestamp), in which case the reducer will be scheduled once, or from a [`TimeDuration`](#struct-timeduration), in which case the reducer will be scheduled in a loop.

Example:

```csharp
using SpacetimeDB;

public static partial class Module
{

    // First, we declare the table with scheduling information.

    [Table(Name = "send_message_schedule", Scheduled = nameof(SendMessage), ScheduledAt = nameof(ScheduledAt))]
    public partial struct SendMessageSchedule
    {

        // Mandatory fields:

        [PrimaryKey]
        [AutoInc]
        public ulong Id;

        public ScheduleAt ScheduledAt;

        // Custom fields:

        public string Message;
    }

    // Then, we declare the scheduled reducer.
    // The first argument of the reducer should be, as always, a `ReducerContext`.
    // The second argument should be a row of the scheduling information table.

    [Reducer]
    public static void SendMessage(ReducerContext ctx, SendMessageSchedule schedule)
    {
        Log.Info($"Sending message {schedule.Message}");
        // ...
    }

    // Finally, we want to actually start scheduling reducers.
    // It's convenient to do this inside the `init` reducer.

    [Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        var currentTime = ctx.Timestamp;
        var tenSeconds = new TimeDuration { Microseconds = +10_000_000 };
        var futureTimestamp = currentTime + tenSeconds;

        ctx.Db.send_message_schedule.Insert(new()
        {
            Id = 0, // Have [AutoInc] assign an Id.
            ScheduledAt = new ScheduleAt.Time(futureTimestamp),
            Message = "I'm a bot sending a message one time!"
        });

        ctx.Db.send_message_schedule.Insert(new()
        {
            Id = 0, // Have [AutoInc] assign an Id.
            ScheduledAt = new ScheduleAt.Interval(tenSeconds),
            Message = "I'm a bot sending a message every ten seconds!"
        });
    }
}
```

Scheduled reducers are called on a best-effort basis and may be slightly delayed in their execution
when a database is under heavy load.

#### Restricting scheduled reducers

Scheduled reducers are normal reducers, and may still be called by clients.
If a scheduled reducer should only be called by the scheduler, consider beginning it with a check that the caller `Identity` is the module:

```csharp
[Reducer]
public static void SendMessage(ReducerContext ctx, SendMessageSchedule schedule)
{
    if (ctx.Sender != ctx.Identity)
    {
        throw new Exception("Reducer SendMessage may not be invoked by clients, only via scheduling.");
    }
    // ...
}
```

## Automatic migrations

When you `spacetime publish` a module that has already been published using `spacetime publish <DATABASE_NAME_OR_IDENTITY>`,
SpacetimeDB attempts to automatically migrate your existing database to the new schema. (The "schema" is just the collection
of tables and reducers you've declared in your code, together with the types they depend on.) This form of migration is limited and only supports a few kinds of changes.
On the plus side, automatic migrations usually don't break clients. The situations that may break clients are documented below.

The following changes are always allowed and never breaking:

- ✅ **Adding tables**. Non-updated clients will not be able to see the new tables.
- ✅ **Adding indexes**.
- ✅ **Adding or removing `[AutoInc]` annotations.**
- ✅ **Changing tables from private to public**.
- ✅ **Adding reducers**.
- ✅ **Removing `[Unique]` annotations.**

The following changes are allowed, but may break clients:

- ⚠️ **Changing or removing reducers**. Clients that attempt to call the old version of a changed reducer will receive runtime errors.
- ⚠️ **Changing tables from public to private**. Clients that are subscribed to a newly-private table will receive runtime errors.
- ⚠️ **Removing `[PrimaryKey]` annotations**. Non-updated clients will still use the old `[PrimaryKey]` as a unique key in their local cache, which can result in non-deterministic behavior when updates are received.
- ⚠️ **Removing indexes**. This is only breaking in some situtations.
  The specific problem is subscription queries <!-- TODO: clientside link --> involving semijoins, such as:

  ```sql
  SELECT Employee.*
  FROM Employee JOIN Dept
  ON Employee.DeptName = Dept.DeptName
  )
  ```

  For performance reasons, SpacetimeDB will only allow this kind of subscription query if there are indexes on `Employee.DeptName` and `Dept.DeptName`. Removing either of these indexes will invalidate this subscription query, resulting in client-side runtime errors.

The following changes are forbidden without a manual migration:

- ❌ **Removing tables**.
- ❌ **Changing the columns of a table**. This includes changing the order of columns of a table.
- ❌ **Changing whether a table is used for [scheduling](#scheduled-reducers).** <!-- TODO: update this if we ever actually implement it... -->
- ❌ **Adding `[Unique]` or `[PrimaryKey]` constraints.** This could result in existing tables being in an invalid state.

Currently, manual migration support is limited. The `spacetime publish --clear-database <DATABASE_IDENTITY>` command can be used to **COMPLETELY DELETE** and reinitialize your database, but naturally it should be used with EXTREME CAUTION.

## Other infrastructure

### Class `Log`

```csharp
namespace SpacetimeDB
{
    public static class Log
    {
        public static void Debug(string message);
        public static void Error(string message);
        public static void Exception(string message);
        public static void Exception(Exception exception);
        public static void Info(string message);
        public static void Trace(string message);
        public static void Warn(string message);
    }
}
```

Methods for writing to a private debug log. Log messages will include file and line numbers.

Log outputs of a running database can be inspected using the `spacetime logs` command:

```text
spacetime logs <DATABASE_IDENTITY>
```

These are only visible to the database owner, not to clients or other developers.

Note that `Log.Error` and `Log.Exception` only write to the log, they do not throw exceptions themselves.

Example:

```csharp
using SpacetimeDB;

public static partial class Module {
    [Table(Name = "user")]
    public partial struct User {
        [PrimaryKey]
        uint Id;
        [Unique]
        string Username;
        ulong DogCount;
    }

    [Reducer]
    public static void LogDogs(ReducerContext ctx) {
        Log.Info("Examining users.");

        var totalDogCount = 0;

        foreach (var user in ctx.Db.user.Iter()) {
            Log.Info($"    User: Id = {user.Id}, Username = {user.Username}, DogCount = {user.DogCount}");

            totalDogCount += user.DogCount;
        }

        if (totalDogCount < 300) {
            Log.Warn("Insufficient dogs.");
        }

        if (totalDogCount < 100) {
            Log.Error("Dog population is critically low!");
        }
    }
}
```

### Attribute `[SpacetimeDB.Type]`

This attribute makes types self-describing, allowing them to automatically register their structure
with SpacetimeDB. Any C# type annotated with `[SpacetimeDB.Type]` can be used as a table column or reducer argument.

Types marked `[SpacetimeDB.Table]` are automatically marked `[SpacetimeDB.Type]`.

`[SpacetimeDB.Type]` can be combined with [`SpacetimeDB.TaggedEnum`] to use tagged enums in tables or reducers.

```csharp
using SpacetimeDB;

public static partial class Module {

    [Type]
    public partial struct Coord {
        public int X;
        public int Y;
    }

    [Type]
    public partial struct TankData {
        public int Ammo;
        public int LeftTreadHealth;
        public int RightTreadHealth;
    }

    [Type]
    public partial struct TransportData {
        public int TroopCount;
    }

    // A type that could be either the data for a Tank or the data for a Transport.
    // See SpacetimeDB.TaggedEnum docs.
    [Type]
    public partial record VehicleData : TaggedEnum<(TankData Tank, TransportData Transport)> {}

    [Table(Name = "vehicle")]
    public partial struct Vehicle {
        [PrimaryKey]
        [AutoInc]
        public uint Id;
        public Coord Coord;
        public VehicleData Data;
    }

    [SpacetimeDB.Reducer]
    public static void InsertVehicle(ReducerContext ctx, Coord Coord, VehicleData Data) {
        ctx.Db.vehicle.Insert(new Vehicle { Id = 0, Coord = Coord, Data = Data });
    }
}
```

The fields of the struct/enum must also be marked with `[SpacetimeDB.Type]`.

Some types from the standard library are also considered to be marked with `[SpacetimeDB.Type]`, including:

- `byte`
- `sbyte`
- `ushort`
- `short`
- `uint`
- `int`
- `ulong`
- `long`
- `SpacetimeDB.U128`
- `SpacetimeDB.I128`
- `SpacetimeDB.U256`
- `SpacetimeDB.I256`
- `List<T>` where `T` is a `[SpacetimeDB.Type]`

### Struct `Identity`

```csharp
namespace SpacetimeDB;

public readonly record struct Identity
{
    public static Identity FromHexString(string hex);
    public string ToString();
}
```

An `Identity` for something interacting with the database.

This is a record struct, so it can be printed, compared with `==`, and used as a `Dictionary` key.

`ToString()` returns a hex encoding of the Identity, suitable for printing.

<!-- TODO: docs for OpenID stuff. -->

### Struct `ConnectionId`

```csharp
namespace SpacetimeDB;

public readonly record struct ConnectionId
{
    public static ConnectionId? FromHexString(string hex);
    public string ToString();
}
```

A unique identifier for a client connection to a SpacetimeDB database.

This is a record struct, so it can be printed, compared with `==`, and used as a `Dictionary` key.

`ToString()` returns a hex encoding of the `ConnectionId`, suitable for printing.

### Struct `Timestamp`

```csharp
namespace SpacetimeDB;

public record struct Timestamp(long MicrosecondsSinceUnixEpoch)
    : IStructuralReadWrite,
        IComparable<Timestamp>
{
    // ...
}
```

A point in time, measured in microseconds since the Unix epoch.
This can be converted to/from a standard library [`DateTimeOffset`]. It is provided for consistency of behavior between SpacetimeDB's supported module and SDK languages.

| Name                                  | Description                                           |
| ------------------------------------- | ----------------------------------------------------- |
| Property `MicrosecondsSinceUnixEpoch` | Microseconds since the [unix epoch].                  |
| Conversion to/from `DateTimeOffset`   | Convert to/from a standard library [`DateTimeOffset`] |
| Static property `UNIX_EPOCH`          | The [unix epoch] as a `Timestamp`                     |
| Method `TimeDurationSince`            | Measure the time elapsed since another `Timestamp`    |
| Operator `+`                          | Add a [`TimeDuration`] to a `Timestamp`               |
| Method `CompareTo`                    | Compare to another `Timestamp`                        |

#### Property `Timestamp.MicrosecondsSinceUnixEpoch`

```csharp
long MicrosecondsSinceUnixEpoch;
```

The number of microseconds since the [unix epoch].

A positive value means a time after the Unix epoch, and a negative value means a time before.

#### Conversion to/from `DateTimeOffset`

```csharp
public static implicit operator DateTimeOffset(Timestamp t);
public static implicit operator Timestamp(DateTimeOffset offset);
```

`Timestamp` may be converted to/from a [`DateTimeOffset`], but the conversion can lose precision.
This type has less precision than DateTimeOffset (units of microseconds rather than units of 100ns).

#### Static property `Timestamp.UNIX_EPOCH`

```csharp
public static readonly Timestamp UNIX_EPOCH = new Timestamp { MicrosecondsSinceUnixEpoch = 0 };
```

The [unix epoch] as a `Timestamp`.

#### Method `Timestamp.TimeDurationSince`

```csharp
public readonly TimeDuration TimeDurationSince(Timestamp earlier) =>
```

Create a new [`TimeDuration`] that is the difference between two `Timestamps`.

#### Operator `Timestamp.+`

```csharp
public static Timestamp operator +(Timestamp point, TimeDuration interval);
```

Create a new `Timestamp` that occurs `interval` after `point`.

#### Method `Timestamp.CompareTo`

```csharp
public int CompareTo(Timestamp that)
```

Compare two `Timestamp`s.

### Struct `TimeDuration`

```csharp
namespace SpacetimeDB;

public record struct TimeDuration(long Microseconds) : IStructuralReadWrite {
    // ...
}
```

A duration that represents an interval between two [`Timestamp`]s.

This type may be converted to/from a [`TimeSpan`]. It is provided for consistency of behavior between SpacetimeDB's supported module and SDK languages.

| Name                                                          | Description                                       |
| ------------------------------------------------------------- | ------------------------------------------------- |
| Property [`Microseconds`](#property-timedurationmicroseconds) | Microseconds between the [`Timestamp`]s.          |
| [Conversion to/from `TimeSpan`](#conversion-tofrom-timespan)  | Convert to/from a standard library [`TimeSpan`]   |
| Static property [`ZERO`](#static-property-timedurationzero)   | The duration between any [`Timestamp`] and itself |

#### Property `TimeDuration.Microseconds`

```csharp
long Microseconds;
```

The number of microseconds between two [`Timestamp`]s.

#### Conversion to/from `TimeSpan`

```csharp
public static implicit operator TimeSpan(TimeDuration d) =>
    new(d.Microseconds * Util.TicksPerMicrosecond);

public static implicit operator TimeDuration(TimeSpan timeSpan) =>
    new(timeSpan.Ticks / Util.TicksPerMicrosecond);
```

`TimeDuration` may be converted to/from a [`TimeSpan`], but the conversion can lose precision.
This type has less precision than [`TimeSpan`] (units of microseconds rather than units of 100ns).

#### Static property `TimeDuration.ZERO`

```csharp
public static readonly TimeDuration ZERO = new TimeDuration { Microseconds = 0 };
```

The duration between any `Timestamp` and itself.

### Record `TaggedEnum`

```csharp
namespace SpacetimeDB;

public abstract record TaggedEnum<Variants> : IEquatable<TaggedEnum<Variants>> where Variants : struct, ITuple
```

A [tagged enum](https://en.wikipedia.org/wiki/Tagged_union) is a type that can hold a value from any one of several types. `TaggedEnum` uses code generation to accomplish this.

For example, to declare a type that can be either a `string` or an `int`, write:

```csharp
[SpacetimeDB.Type]
public partial record ProductId : SpacetimeDB.TaggedEnum<(string Text, uint Number)> { }
```

Here there are two **variants**: one is named `Text` and holds a `string`, the other is named `Number` and holds a `uint`.

To create a value of this type, use `new {Type}.{Variant}({data})`. For example:

```csharp
ProductId a = new ProductId.Text("apple");
ProductId b = new ProductId.Number(57);
ProductId c = new ProductId.Number(59);
```

To use a value of this type, you need to check which variant it stores.
This is done with [C# pattern matching syntax](https://learn.microsoft.com/en-us/dotnet/csharp/fundamentals/functional/pattern-matching). For example:

```csharp
public static void Print(ProductId id)
{
    if (id is ProductId.Text(var s))
    {
        Log.Info($"Textual product ID: '{s}'");
    }
    else if (id is ProductId.Number(var i))
    {
        Log.Info($"Numeric Product ID: {i}");
    }
}
```

A `TaggedEnum` can have up to 255 variants, and the variants can be any type marked with [`[SpacetimeDB.Type]`].

```csharp
[SpacetimeDB.Type]
public partial record ManyChoices : SpacetimeDB.TaggedEnum<(
    string String,
    int Int,
    List<int> IntList,
    Banana Banana,
    List<List<Banana>> BananaMatrix
)> { }

[SpacetimeDB.Type]
public partial struct Banana {
    public int Sweetness;
    public int Rot;
}
```

`TaggedEnums` are an excellent alternative to nullable fields when groups of fields are always set together. Consider a data type like:

```csharp
[SpacetimeDB.Type]
public partial struct ShapeData {
    public int? CircleRadius;
    public int? RectWidth;
    public int? RectHeight;
}
```

Often this is supposed to be a circle XOR a rectangle -- that is, not both at the same time. If this is the case, then we don't want to set `circleRadius` at the same time as `rectWidth` or `rectHeight`. Also, if `rectWidth` is set, we expect `rectHeight` to be set.
However, C# doesn't know about this, so code using this type will be littered with extra null checks.

If we instead write:

```csharp
[SpacetimeDB.Type]
public partial struct CircleData {
    public int Radius;
}

[SpacetimeDB.Type]
public partial struct RectData {
    public int Width;
    public int Height;
}

[SpacetimeDB.Type]
public partial record ShapeData : SpacetimeDB.TaggedEnum<(CircleData Circle, RectData Rect)> { }
```

Then code using a `ShapeData` will only have to do one check -- do I have a circle or a rectangle?
And in each case, the data will be guaranteed to have exactly the fields needed.

### Record `ScheduleAt`

```csharp
namespace SpacetimeDB;

public partial record ScheduleAt : TaggedEnum<(TimeDuration Interval, Timestamp Time)>
```

When a [scheduled reducer](#scheduled-reducers) should execute, either at a specific point in time, or at regular intervals for repeating schedules.

Stored in reducer-scheduling tables as a column.

[client]: https://spacetimedb.com/docs/#client
[clients]: https://spacetimedb.com/docs/#client
[client SDK documentation]: https://spacetimedb.com/docs/#client
[`DateTimeOffset`]: https://learn.microsoft.com/en-us/dotnet/api/system.datetimeoffset?view=net-9.0
[`TimeSpan`]: https://learn.microsoft.com/en-us/dotnet/api/system.timespan?view=net-9.0
[unix epoch]: https://en.wikipedia.org/wiki/Unix_time
[`System.Random`]: https://learn.microsoft.com/en-us/dotnet/api/system.random?view=net-9.0
