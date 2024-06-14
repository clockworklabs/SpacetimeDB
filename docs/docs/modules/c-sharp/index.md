# SpacetimeDB C# Modules

You can use the [C# SpacetimeDB library](https://github.com/clockworklabs/SpacetimeDBLibCSharp) to write modules in C# which interact with the SpacetimeDB database.

It uses [Roslyn incremental generators](https://github.com/dotnet/roslyn/blob/main/docs/features/incremental-generators.md) to add extra static methods to types, tables and reducers marked with special attributes and registers them with the database runtime.

## Example

Let's start with a heavily commented version of the default example from the landing page:

```csharp
// These imports bring into the scope common APIs you'll need to expose items from your module and to interact with the database runtime.
using SpacetimeDB.Module;
using static SpacetimeDB.Runtime;

// Roslyn generators are statically generating extra code as-if they were part of the source tree, so,
// in order to inject new methods, types they operate on as well as their parents have to be marked as `partial`.
//
// We start with the top-level `Module` class for the module itself.
static partial class Module
{
    // `[SpacetimeDB.Table]` registers a struct or a class as a SpacetimeDB table.
    //
    // It generates methods to insert, filter, update, and delete rows of the given type in the table.
    [SpacetimeDB.Table(Public = true)]
    public partial struct Person
    {
        // `[SpacetimeDB.Column]` allows to specify column attributes / constraints such as
        // "this field should be unique" or "this field should get automatically assigned auto-incremented value".
        [SpacetimeDB.Column(ColumnAttrs.Unique | ColumnAttrs.AutoInc)]
        public int Id;
        public string Name;
        public int Age;
    }

    // `[SpacetimeDB.Reducer]` marks a static method as a SpacetimeDB reducer.
    //
    // Reducers are functions that can be invoked from the database runtime.
    // They can't return values, but can throw errors that will be caught and reported back to the runtime.
    [SpacetimeDB.Reducer]
    public static void Add(string name, int age)
    {
        // We can skip (or explicitly set to zero) auto-incremented fields when creating new rows.
        var person = new Person { Name = name, Age = age };
        
        // `Insert()` method is auto-generated and will insert the given row into the table.
        person.Insert();
        // After insertion, the auto-incremented fields will be populated with their actual values.
        //
        // `Log()` function is provided by the runtime and will print the message to the database log.
        // It should be used instead of `Console.WriteLine()` or similar functions.
        Log($"Inserted {person.Name} under #{person.Id}");
    }

    [SpacetimeDB.Reducer]
    public static void SayHello()
    {
        // Each table type gets a static Iter() method that can be used to iterate over the entire table.
        foreach (var person in Person.Iter())
        {
            Log($"Hello, {person.Name}!");
        }
        Log("Hello, World!");
    }
}
```

## API reference

Now we'll get into details on all the APIs SpacetimeDB provides for writing modules in C#.

### Logging

First of all, logging as we're likely going to use it a lot for debugging and reporting errors.

`SpacetimeDB.Runtime` provides a `Log` function that will print the given message to the database log, along with the source location and a log level it was provided.

Supported log levels are provided by the `LogLevel` enum:

```csharp
public enum LogLevel
{
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    Panic
}
```

If omitted, the log level will default to `Info`, so these two forms are equivalent:

```csharp
Log("Hello, World!");
Log("Hello, World!", LogLevel.Info);
```

### Supported types

#### Built-in types

The following types are supported out of the box and can be stored in the database tables directly or as part of more complex types:

- `bool`
- `byte`, `sbyte`
- `short`, `ushort`
- `int`, `uint`
- `long`, `ulong`
- `float`, `double`
- `string`
- [`Int128`](https://learn.microsoft.com/en-us/dotnet/api/system.int128), [`UInt128`](https://learn.microsoft.com/en-us/dotnet/api/system.uint128)
- `T[]` - arrays of supported values.
- [`List<T>`](https://learn.microsoft.com/en-us/dotnet/api/system.collections.generic.list-1)
- [`Dictionary<TKey, TValue>`](https://learn.microsoft.com/en-us/dotnet/api/system.collections.generic.dictionary-2)

And a couple of special custom types:

- `SpacetimeDB.SATS.Unit` - semantically equivalent to an empty struct, sometimes useful in generic contexts where C# doesn't permit `void`.
- `Identity` (`SpacetimeDB.Runtime.Identity`) - a unique identifier for each user; internally a byte blob but can be printed, hashed and compared for equality.
- `Address` (`SpacetimeDB.Runtime.Address`) - an identifier which disamgibuates connections by the same `Identity`; internally a byte blob but can be printed, hashed and compared for equality.


#### Custom types

`[SpacetimeDB.Type]` attribute can be used on any `struct`, `class` or an `enum` to mark it as a SpacetimeDB type. It will implement serialization and deserialization for values of this type so that they can be stored in the database.

Any `struct` or `class` marked with this attribute, as well as their respective parents, must be `partial`, as the code generator will add methods to them.

```csharp
[SpacetimeDB.Type]
public partial struct Point
{
    public int x;
    public int y;
}
```

`enum`s marked with this attribute must not use custom discriminants, as the runtime expects them to be always consecutive starting from zero. Unlike structs and classes, they don't use `partial` as C# doesn't allow to add methods to `enum`s.

```csharp
[SpacetimeDB.Type]
public enum Color
{
    Red,
    Green,
    Blue,
}
```

#### Tagged enums

SpacetimeDB has support for tagged enums which can be found in languages like Rust, but not C#.

To bridge the gap, a special marker interface `SpacetimeDB.TaggedEnum` can be used on any `SpacetimeDB.Type`-marked `struct` or `class` to mark it as a SpacetimeDB tagged enum. It accepts a tuple of 2 or more named items and will generate methods to check which variant is currently active, as well as accessors for each variant.

It is expected that you will use the `Is*` methods to check which variant is active before accessing the corresponding field, as the accessor will throw an exception on a state mismatch.

```csharp
// Example declaration:
[SpacetimeDB.Type]
partial struct Option<T> : SpacetimeDB.TaggedEnum<(T Some, Unit None)> { }

// Usage:
var option = new Option<int> { Some = 42 };
if (option.IsSome)
{
    Log($"Value: {option.Some}");
}
```

### Tables

`[SpacetimeDB.Table]` attribute can be used on any `struct` or `class` to mark it as a SpacetimeDB table. It will register a table in the database with the given name and fields as well as will generate C# methods to insert, filter, update, and delete rows of the given type.
By default, tables are **private**. This means that they are only readable by the table owner, and by server module code.
Adding `[SpacetimeDB.Table(Public = true))]` annotation makes a table public. **Public** tables are readable by all users, but can still only be modified by your server module code.

_Coming soon: We plan to add much more robust access controls than just public or private. Stay tuned!_

It implies `[SpacetimeDB.Type]`, so you must not specify both attributes on the same type.

```csharp
[SpacetimeDB.Table(Public = true)]
public partial struct Person
{
    [SpacetimeDB.Column(ColumnAttrs.Unique | ColumnAttrs.AutoInc)]
    public int Id;
    public string Name;
    public int Age;
}
```

The example above will generate the following extra methods:

```csharp
public partial struct Person
{
    // Inserts current instance as a new row into the table.
    public void Insert();

    // Returns an iterator over all rows in the table, e.g.:
    // `for (var person in Person.Iter()) { ... }`
    public static IEnumerable<Person> Iter();

    // Returns an iterator over all rows in the table that match the given filter, e.g.:
    // `for (var person in Person.Query(p => p.Age >= 18)) { ... }`
    public static IEnumerable<Person> Query(Expression<Func<Person, bool>> filter);

    // Generated for each column:

    // Returns an iterator over all rows in the table that have the given value in the `Name` column.
    public static IEnumerable<Person> FilterByName(string name);
    public static IEnumerable<Person> FilterByAge(int age);

    // Generated for each unique column:

    // Finds a row in the table with the given value in the `Id` column and returns it, or `null` if no such row exists.
    public static Person? FindById(int id);
    
    // Deletes a row in the table with the given value in the `Id` column and returns `true` if the row was found and deleted, or `false` if no such row exists.
    public static bool DeleteById(int id);
    
    // Updates a row in the table with the given value in the `Id` column and returns `true` if the row was found and updated, or `false` if no such row exists.
    public static bool UpdateById(int oldId, Person newValue);
}
```

#### Column attributes

Attribute `[SpacetimeDB.Column]` can be used on any field of a `SpacetimeDB.Table`-marked `struct` or `class` to customize column attributes as seen above.

The supported column attributes are:

- `ColumnAttrs.AutoInc` - this column should be auto-incremented.
- `ColumnAttrs.Unique` - this column should be unique.
- `ColumnAttrs.PrimaryKey` - this column should be a primary key, it implies `ColumnAttrs.Unique` but also allows clients to subscribe to updates via `OnUpdate` which will use this field to match the old and the new version of the row with each other.

These attributes are bitflags and can be combined together, but you can also use some predefined shortcut aliases:

- `ColumnAttrs.Identity` - same as `ColumnAttrs.Unique | ColumnAttrs.AutoInc`.
- `ColumnAttrs.PrimaryKeyAuto` - same as `ColumnAttrs.PrimaryKey | ColumnAttrs.AutoInc`.

### Reducers

Attribute `[SpacetimeDB.Reducer]` can be used on any `static void` method to register it as a SpacetimeDB reducer. The method must accept only supported types as arguments. If it throws an exception, those will be caught and reported back to the database runtime.

```csharp
[SpacetimeDB.Reducer]
public static void Add(string name, int age)
{
    var person = new Person { Name = name, Age = age };
    person.Insert();
    Log($"Inserted {person.Name} under #{person.Id}");
}
```

If a reducer has an argument with a type `DbEventArgs` (`SpacetimeDB.Runtime.DbEventArgs`), it will be provided with event details such as the sender identity (`SpacetimeDB.Runtime.Identity`), sender address (`SpacetimeDB.Runtime.Address?`) and the time (`DateTimeOffset`) of the invocation:

```csharp
[SpacetimeDB.Reducer]
public static void PrintInfo(DbEventArgs e)
{
    Log($"Sender identity: {e.Sender}");
    Log($"Sender address: {e.Address}");
    Log($"Time: {e.Time}");
}
```

`[SpacetimeDB.Reducer]` also generates a function to schedule the given reducer in the future.

Since it's not possible to generate extension methods on existing methods, the codegen will instead add a `Schedule`-prefixed method colocated in the same namespace as the original method instead. The generated method will accept `DateTimeOffset` argument for the time when the reducer should be invoked, followed by all the arguments of the reducer itself, except those that have type `DbEventArgs`.

```csharp
// Example reducer:
[SpacetimeDB.Reducer]
public static void Add(string name, int age) { ... }

// Auto-generated by the codegen:
public static void ScheduleAdd(DateTimeOffset time, string name, int age) { ... }

// Usage from another reducer:
[SpacetimeDB.Reducer]
public static void AddIn5Minutes(DbEventArgs e, string name, int age)
{
    // Note that we're using `e.Time` instead of `DateTimeOffset.Now` which is not allowed in modules.
    var scheduleToken = ScheduleAdd(e.Time.AddMinutes(5), name, age);

    // We can cancel the scheduled reducer by calling `Cancel()` on the returned token.
    scheduleToken.Cancel();
}
```

#### Special reducers

These are two special kinds of reducers that can be used to respond to module lifecycle events. They're stored in the `SpacetimeDB.Module.ReducerKind` class and can be used as an argument to the `[SpacetimeDB.Reducer]` attribute:

- `ReducerKind.Init` - this reducer will be invoked when the module is first published.
- `ReducerKind.Update` - this reducer will be invoked when the module is updated.
- `ReducerKind.Connect` - this reducer will be invoked when a client connects to the database.
- `ReducerKind.Disconnect` - this reducer will be invoked when a client disconnects from the database.


Example:

```csharp
[SpacetimeDB.Reducer(ReducerKind.Init)]
public static void Init()
{
    Log("...and we're live!");
}
```
