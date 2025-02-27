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

We provide a tagged enum support for C# modules via a special `record SpacetimeDB.TaggedEnum<(...types and names of the variants as a tuple...)>`.

When you inherit from the `SpacetimeDB.TaggedEnum` marker, it will generate variants as subclasses of the annotated type, so you can use regular C# pattern matching operators like `is` or `switch` to determine which variant a given tagged enum holds at any time.

For unit variants (those without any data payload) you can use a built-in `SpacetimeDB.Unit` as the variant type.

Example:

```csharp
// Define a tagged enum named `MyEnum` with three variants,
// `MyEnum.String`, `MyEnum.Int` and `MyEnum.None`.
[SpacetimeDB.Type]
public partial record MyEnum : SpacetimeDB.TaggedEnum<(
    string String,
    int Int,
    SpacetimeDB.Unit None
)>;

// Print an instance of `MyEnum`, using `switch`/`case` to determine the active variant.
void PrintEnum(MyEnum e)
{
    switch (e)
    {
        case MyEnum.String(var s):
            Console.WriteLine(s);
            break;

        case MyEnum.Int(var i):
            Console.WriteLine(i);
            break;

        case MyEnum.None:
            Console.WriteLine("(none)");
            break;
    }
}

// Test whether an instance of `MyEnum` holds some value (either a string or an int one).
bool IsSome(MyEnum e) => e is not MyEnum.None;

// Construct an instance of `MyEnum` with the `String` variant active.
var myEnum = new MyEnum.String("Hello, world!");
Console.WriteLine($"IsSome: {IsSome(myEnum)}");
PrintEnum(myEnum);
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

You can create multiple tables backed by items of the same type by applying it with different names. For example, to store active and archived posts separately and with different privacy rules, you can declare two tables like this:

```csharp
[SpacetimeDB.Table(Name = "Post", Public = true)]
[SpacetimeDB.Table(Name = "ArchivedPost", Public = false)]
public partial struct Post {
    public string Title;
    public string Body;
}
```

#### Column attributes

Attribute `[SpacetimeDB.Column]` can be used on any field of a `SpacetimeDB.Table`-marked `struct` or `class` to customize column attributes as seen above.

The supported column attributes are:

- `ColumnAttrs.AutoInc` - this column should be auto-incremented.

**Note**: The `AutoInc` number generator is not transactional. See the [SEQUENCE] section for more details.

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

If a reducer has an argument with a type `ReducerContext` (`SpacetimeDB.Runtime.ReducerContext`), it will be provided with event details such as the sender identity (`SpacetimeDB.Runtime.Identity`), sender address (`SpacetimeDB.Runtime.Address?`) and the time (`DateTimeOffset`) of the invocation:

```csharp
[SpacetimeDB.Reducer]
public static void PrintInfo(ReducerContext e)
{
    Log($"Sender identity: {e.Sender}");
    Log($"Sender address: {e.Address}");
    Log($"Time: {e.Time}");
}
```

### Scheduler Tables

Tables can be used to schedule a reducer calls either at a specific timestamp or at regular intervals.

```csharp
public static partial class Timers
{

    // The `Scheduled` attribute links this table to a reducer.
    [SpacetimeDB.Table(Scheduled = nameof(SendScheduledMessage))]
    public partial struct SendMessageTimer
    {
        public string Text;
    }


    // Define the reducer that will be invoked by the scheduler table.
    // The first parameter is always `ReducerContext`, and the second parameter is an instance of the linked table struct.
    [SpacetimeDB.Reducer]
    public static void SendScheduledMessage(ReducerContext ctx, SendMessageTimer arg)
    {
        // ...
    }


    // Scheduling reducers inside `init` reducer.
    [SpacetimeDB.Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {

        // Schedule a one-time reducer call by inserting a row.
        new SendMessageTimer
        {
            Text = "bot sending a message",
            ScheduledAt = ctx.Time.AddSeconds(10),
            ScheduledId = 1,
        }.Insert();


        // Schedule a recurring reducer.
        new SendMessageTimer
        {
            Text = "bot sending a message",
            ScheduledAt = new TimeStamp(10),
            ScheduledId = 2,
        }.Insert();
    }
}
```

Annotating a struct with `Scheduled` automatically adds fields to support scheduling, It can be expanded as:

```csharp
public static partial class Timers
{
    [SpacetimeDB.Table]
    public partial struct SendMessageTimer
    {
        public string Text;         // fields of original struct

        [SpacetimeDB.Column(ColumnAttrs.PrimaryKeyAuto)]
        public ulong ScheduledId;   // unique identifier to be used internally

        public SpacetimeDB.ScheduleAt ScheduleAt;   // Scheduling details (Time or Inteval)
    }
}

// `ScheduledAt` definition
public abstract partial record ScheduleAt: SpacetimeDB.TaggedEnum<(DateTimeOffset Time, TimeSpan Interval)>
```

#### Special reducers

These are four special kinds of reducers that can be used to respond to module lifecycle events. They're stored in the `SpacetimeDB.Module.ReducerKind` class and can be used as an argument to the `[SpacetimeDB.Reducer]` attribute:

- `ReducerKind.Init` - this reducer will be invoked when the module is first published.
- `ReducerKind.Update` - this reducer will be invoked when the module is updated.
- `ReducerKind.Connect` - this reducer will be invoked when a client connects to the database.
- `ReducerKind.Disconnect` - this reducer will be invoked when a client disconnects from the database.

Example:

````csharp
[SpacetimeDB.Reducer(ReducerKind.Init)]
public static void Init()
{
    Log("...and we're live!");
}

[SpacetimeDB.Reducer(ReducerKind.Update)]
public static void Update()
{
    Log("Update get!");
}

[SpacetimeDB.Reducer(ReducerKind.Connect)]
public static void OnConnect(DbEventArgs ctx)
{
    Log($"{ctx.Sender} has connected from {ctx.Address}!");
}

[SpacetimeDB.Reducer(ReducerKind.Disconnect)]
public static void OnDisconnect(DbEventArgs ctx)
{
    Log($"{ctx.Sender} has disconnected.");
}```
````

[SEQUENCE]: /docs/appendix#sequence