namespace SpacetimeDB.Examples.QuickStart.Server;

using SpacetimeDB;

[Table(Name = "person", Public = true)]
public partial struct Person
{
    [AutoInc]
    [PrimaryKey]
    public uint id;
    public string name;

    [Index.BTree]
    public byte age;
}

static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void add(ReducerContext ctx, string name, byte age)
    {
        ctx.Db.person.Insert(new Person { name = name, age = age });
    }

    [SpacetimeDB.Reducer]
    public static void say_hello(ReducerContext ctx)
    {
        foreach (var person in ctx.Db.person.Iter())
        {
            Log.Info($"Hello, {person.name}!");
        }
        Log.Info("Hello, World!");
    }

    [SpacetimeDB.Reducer]
    public static void list_over_age(ReducerContext ctx, byte age)
    {
        foreach (var person in ctx.Db.person.age.Filter((age, byte.MaxValue)))
        {
            Log.Info($"{person.name} has age {person.age} >= {age}");
        }
    }
}
