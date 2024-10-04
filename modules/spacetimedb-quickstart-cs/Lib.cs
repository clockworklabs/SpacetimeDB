namespace SpacetimeDB.Examples.QuickStart.Server;

using SpacetimeDB;

[Table(Public = true)]
[Index(Name = "Age", BTree = ["Age"])]
public partial struct Person
{
    [AutoInc]
    [PrimaryKey]
    public uint Id;
    public string Name;
    public byte Age;
}

static partial class Module
{
    [SpacetimeDB.Reducer]
    public static void add(ReducerContext ctx, string name, byte age)
    {
        ctx.Db.Person.Insert(new Person { Name = name, Age = age });
    }

    [SpacetimeDB.Reducer]
    public static void say_hello(ReducerContext ctx)
    {
        foreach (var person in ctx.Db.Person.Iter())
        {
            Log.Info($"Hello, {person.Name}!");
        }
        Log.Info("Hello, World!");
    }

    [SpacetimeDB.Reducer]
    public static void list_over_age(ReducerContext ctx, byte age)
    {
        foreach (var person in ctx.Db.Person.Age.Filter((age, byte.MaxValue)))
        {
            Log.Info($"{person.Name} has age {person.Age} >= {age}");
        }
    }
}
