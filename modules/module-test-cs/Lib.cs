namespace SpacetimeDB.Modules.ModuleTestCs;

using SpacetimeDB;

// A C# type alias for TestA.
using TestAlias = TestA;

// ─────────────────────────────────────────────────────────────────────────────
// TABLE DEFINITIONS
// ─────────────────────────────────────────────────────────────────────────────

[Table(Name = "person", Public = true)]
public partial struct Person
{
    [PrimaryKey]
    [AutoInc]
    public uint id;
    public string name;

    [Index.BTree]
    public byte age;
}

[Table(Name = "test_a")]
public partial struct TestA
{
    // The index on column "x" is given the name "foo".
    [Index.BTree(Name = "foo")]
    public uint x;
    public uint y;
    public string z;
}

// A type used only for data (no table attribute).
[Type]
public partial struct TestB
{
    public string foo;
}

[Type]
// TODO(cloutiertyler): Fix this when it is supported.
// [Sats(Name = "Namespace.TestC")]
public enum TestC
{
    Foo,
    Bar
}

[Table(Name = "test_d", Public = true)]
public partial struct TestD
{
    // In Rust this was an Option<TestC>; in C# we use a nullable enum.
    public TestC? test_c;
}

[Table(Name = "test_e")]
public partial struct TestE
{
    [PrimaryKey]
    [AutoInc]
    public ulong id;
    [Index.BTree]
    public string name;
}

[Type]
public partial record Baz
{
    public string field;
}

[Type]
public partial record Bar
{
    // An empty record to represent the unit variant "Bar".
}

[Type]
public partial record Foobar : TaggedEnum<(Baz Baz, Bar Bar, uint Har)>
{
}

[Table(Name = "test_f", Public = true)]
public partial struct TestFoobar
{
    public Foobar field;
}

[Type]
public partial record TestFFoo { }

[Type]
public partial record TestFBar { }

[Type]
public partial record TestFBaz
{
    public string value;
}

[Type]
// TODO(cloutiertyler): Fix this when it is supported.
// [SpacetimeDB.Sats(Name = "Namespace.TestF")]
public partial record TestF : TaggedEnum<(TestFFoo Foo, TestFBar Bar, TestFBaz Baz)>
{
}

// FIXME: Table named "private" doesn't compile in C#
// When you fix me, uncomment the code in module-test
// [Table(Name = "private", Public = true)]
// public partial struct TypeNamedPrivateIsNotTheProblem
// {
//     public string name;
// }

// A table marked as private.
[Table(Name = "private_table", Public = false)]
public partial struct PrivateTable
{
    public string name;
}

// A table with a multi‑column index.
[Table(Name = "points", Public = false)]
[Index.BTree(Name = "multi_column_index", Columns = new[] { "x", "y" })]
public partial struct Point
{
    public long x;
    public long y;
}

[Table(Name = "pk_multi_identity")]
public partial struct PkMultiIdentity
{
    [PrimaryKey]
    public uint id;
    [Unique]
    [AutoInc]
    public uint other;
}

[Table(Name = "repeating_test_arg", Scheduled = nameof(Module.repeating_test), ScheduledAt = nameof(scheduled_at))]
public partial struct RepeatingTestArg
{
    [PrimaryKey]
    [AutoInc]
    public ulong scheduled_id;
    public ScheduleAt scheduled_at;
    public Timestamp prev_time;
}

[Table(Name = "has_special_stuff")]
public partial struct HasSpecialStuff
{
    public Identity identity;
    public ConnectionId connectionId;
}

// Two tables using the same row type.
[Table(Name = "player", Public = true)]
[Table(Name = "logged_out_player", Public = true)]
public partial struct Player
{
    [PrimaryKey]
    public Identity identity;
    [AutoInc]
    [Unique]
    public ulong player_id;
    [Unique]
    public string name;
}

// ─────────────────────────────────────────────────────────────────────────────
// SUPPORT TYPES
// ─────────────────────────────────────────────────────────────────────────────

// We can derive `Deserialize` for lifetime generic types:
public partial class Foo
{
    public string field { get; set; }

    // TODO: Bsatn seems not to be available in C# yet
    // 
    // public static Foo Baz(byte[] data)
    // {
    //     // Assume Bsatn.FromSlice<T> is available in SpacetimeDB.
    //     return Bsatn.FromSlice<Foo>(data);
    // }
}

// ─────────────────────────────────────────────────────────────────────────────
// REDUCERS
// ─────────────────────────────────────────────────────────────────────────────

static partial class Module
{
    // This reducer is run at module initialization.
    [Reducer(ReducerKind.Init)]
    public static void init(ReducerContext ctx)
    {
        ctx.Db.repeating_test_arg.Insert(new RepeatingTestArg
        {
            prev_time = ctx.Timestamp,
            scheduled_id = 0,
            scheduled_at = new TimeDuration(1000000)
        });
    }

    [Reducer]
    public static void repeating_test(ReducerContext ctx, RepeatingTestArg arg)
    {
        var deltaTime = ctx.Timestamp.TimeDurationSince(arg.prev_time);
        Log.Trace($"Timestamp: {ctx.Timestamp}, Delta time: {deltaTime}");
    }

    [Reducer]
    public static void add(ReducerContext ctx, string name, byte age)
    {
        ctx.Db.person.Insert(new Person { id = 0, name = name, age = age });
    }

    [Reducer]
    public static void say_hello(ReducerContext ctx)
    {
        foreach (var person in ctx.Db.person.Iter())
        {
            Log.Info($"Hello, {person.name}!");
        }
        Log.Info("Hello, World!");
    }

    [Reducer]
    public static void list_over_age(ReducerContext ctx, byte age)
    {
        // In C# we assume the BTree index filter accepts a tuple representing a range.
        foreach (var person in ctx.Db.person.age.Filter((age, byte.MaxValue)))
        {
            Log.Info($"{person.name} has age {person.age} >= {age}");
        }
    }

    [Reducer]
    public static void log_module_identity(ReducerContext ctx)
    {
        // Note: converting to lowercase to match the Rust formatting.
        Log.Info($"Module identity: {ctx.Identity.ToString().ToLower()}");
    }

    [Reducer]
    public static void test(ReducerContext ctx, TestAlias arg, TestB arg2, TestC arg3, TestF arg4)
    {
        Log.Info("BEGIN");
        Log.Info($"sender: {ctx.Sender}");
        Log.Info($"timestamp: {ctx.Timestamp}");
        Log.Info($"bar: {arg2.foo}");

        // Handle TestC (a simple enum).
        switch (arg3)
        {
            case TestC.Foo:
                Log.Info("Foo");
                break;
            case TestC.Bar:
                Log.Info("Bar");
                break;
        }

        // Handle TestF (a tagged enum). We pattern‐match on its concrete types.
        switch (arg4)
        {
            case TestF.Foo _:
                Log.Info("Foo");
                break;
            case TestF.Bar _:
                Log.Info("Bar");
                break;
            case TestF.Baz fb:
                Log.Info(fb.Baz_.value);
                break;
        }

        // Insert 1000 rows into the test_a table.
        for (uint i = 0; i < 1000; i++)
        {
            ctx.Db.test_a.Insert(new TestA
            {
                x = i + arg.x,
                y = i + arg.y,
                z = "Yo"
            });
        }

        var rowCountBeforeDelete = ctx.Db.test_a.Count;
        Log.Info($"Row count before delete: {rowCountBeforeDelete}");

        uint numDeleted = 0;
        // Delete rows using the "foo" index (from 5 up to, but not including, 10).
        for (uint row = 5; row < 10; row++)
        {
            // FIXME: Apprently in Rust you can delete by index, but in C# you can't.
            // numDeleted += ctx.Db.test_a.foo.Delete(row);
        }

        var rowCountAfterDelete = ctx.Db.test_a.Count;

        if (rowCountBeforeDelete != rowCountAfterDelete + numDeleted)
        {
            Log.Error($"Started with {rowCountBeforeDelete} rows, deleted {numDeleted}, and wound up with {rowCountAfterDelete} rows... huh?");
        }

        // Try inserting into test_e.
        // FIXME: C# doesn't generate TryInsert methods.
        // var insertResult = ctx.Db.test_e.TryInsert(new TestE
        // {
        //     id = 0,
        //     name = "Tyler"
        // });
        // if (insertResult.IsOk)
        // {
        //     Log.Info($"Inserted: {insertResult.Value}");
        // }
        // else
        // {
        //     Log.Info($"Error: {insertResult.Error}");
        // }

        Log.Info($"Row count after delete: {rowCountAfterDelete}");

        // Here we simply count the rows in test_a again (this could be replaced with a filtered count).
        var otherRowCount = ctx.Db.test_a.Count;
        Log.Info($"Row count filtered by condition: {otherRowCount}");

        Log.Info("MultiColumn");

        // Insert 1000 rows into the points table.
        for (long i = 0; i < 1000; i++)
        {
            ctx.Db.points.Insert(new Point
            {
                x = i + (long)arg.x,
                y = i + (long)arg.y
            });
        }

        // Count rows in points that meet a multi‑column condition.
        var multiRowCount = ctx.Db.points.Iter().Where(row => row.x >= 0 && row.y <= 200).Count();
        Log.Info($"Row count filtered by multi-column condition: {multiRowCount}");

        Log.Info("END");
    }


    [Reducer]
    public static void add_player(ReducerContext ctx, string name)
    {
        // If TryInsert fails it should throw an exception.
        // FIXME: C# doesn't generate TryInsert methods.
        // ctx.Db.test_e.TryInsert(new TestE { id = 0, name = name });
    }

    [Reducer]
    public static void delete_player(ReducerContext ctx, ulong id)
    {
        bool deleted = ctx.Db.test_e.id.Delete(id);
        if (!deleted)
        {
            throw new Exception($"No TestE row with id {id}");
        }
    }

    [Reducer]
    public static void delete_players_by_name(ReducerContext ctx, string name)
    {
        var numDeleted = ctx.Db.test_e.name.Delete(name);
        if (numDeleted == 0)
        {
            throw new Exception($"No TestE row with name {name}");
        }
        else
        {
            Log.Info($"Deleted {numDeleted} player(s) with name {name}");
        }
    }

    [Reducer(ReducerKind.ClientConnected)]
    public static void client_connected(ReducerContext ctx)
    {
        // No operation when a client connects.
    }

    [Reducer]
    public static void add_private(ReducerContext ctx, string name)
    {
        ctx.Db.private_table.Insert(new PrivateTable { name = name });
    }

    [Reducer]
    public static void query_private(ReducerContext ctx)
    {
        foreach (var person in ctx.Db.private_table.Iter())
        {
            Log.Info($"Private, {person.name}!");
        }
        Log.Info("Private, World!");
    }

    [Reducer]
    public static void test_btree_index_args(ReducerContext ctx)
    {
        // Testing various acceptable index filter argument types.
        string s = "String";
        var _1 = ctx.Db.test_e.name.Filter(s);
        var _2 = ctx.Db.test_e.name.Filter("str");

        ctx.Db.test_e.name.Delete(s);
        ctx.Db.test_e.name.Delete("str");

        // For the multi‑column index on points, assume the API offers overloads that accept ranges.
        var mci = ctx.Db.points.multi_column_index;
        var _a = mci.Filter(0L);
        var _b = mci.Filter(0L); // by value or by reference

        // (Assuming that your C# API defines appropriate Range types or overloads.)
        // FIXME(cloutiertyler): C# either doesn't have the ability to do this,
        // or I don't know how to do it. Please bring this section in line with the
        // Rust version when you can.
        // 
        // _ = mci.Filter(new Range<long>(0, 3));
        // _ = mci.Filter(new RangeInclusive<long>(0, 3));
        // _ = mci.Filter(new RangeFrom<long>(0));
        // _ = mci.Filter(new RangeTo<long>(3));
        // _ = mci.Filter(new RangeToInclusive<long>(3));
    }

    [Reducer]
    public static void assert_caller_identity_is_module_identity(ReducerContext ctx)
    {
        var caller = ctx.Sender;
        var owner = ctx.Identity;
        if (!caller.Equals(owner))
        {
            throw new Exception($"Caller {caller} is not the owner {owner}");
        }
        else
        {
            Log.Info($"Called by the owner {owner}");
        }
    }
}
