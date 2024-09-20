namespace SpacetimeDB.Examples.QuickStart.Server;

using SpacetimeDB;

[Table(Public = true)]
public partial struct Person {
    [AutoInc]
    [PrimaryKey]
    public uint Id;
    public string Name;
    public byte Age;
}

static partial class Module
{
    [Reducer("add")]
    public static void Add(ReducerContext ctx, string name, byte age)
    {
        var row = new Person { Name = name, Age = age };
        ctx.Db.Person.Insert(ref row);
    }

    [Reducer("say_hello")]
    public static void SayHello(ReducerContext ctx)
    {
        foreach (var person in ctx.Db.Person.Iter())
        {
            Log.Info($"Hello, {person.Name}!");
        }
        Log.Info("Hello, World!");
    }

    [Reducer("list_over_age")]
    public static void ListOverAge(ReducerContext ctx, byte age)
    {
        foreach (var person in ctx.Db.Person.Query(person => person.Age >= age))
        {
            Log.Info($"{person.Name} has age {person.Age} >= {age}");
        }
    }
}
