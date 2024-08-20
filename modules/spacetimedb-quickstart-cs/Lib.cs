namespace SpacetimeDB.Examples.QuickStart.Server;

using SpacetimeDB;

[Table(Public = true)]
public partial struct Person {
    [PrimaryKey]
    [AutoInc]
    public uint Id;
    public string Name;
    public byte Age;
}

static partial class Module
{
    [Reducer(Name = "add")]
    public static void Add(ReducerContext ctx, string name, byte age) =>
        ctx.Db.Person().Insert(new() { Name = name, Age = age });

    [Reducer(Name = "say_hello")]
    public static void SayHello(ReducerContext ctx)
    {
        foreach (var person in ctx.Db.Person().Iter())
        {
            Runtime.Log($"Hello, {person.Name}!");
        }
        Runtime.Log("Hello, World!");
    }

    [Reducer(Name = "list_over_age")]
    public static void ListOverAge(ReducerContext ctx, byte age)
    {
        foreach (var person in ctx.Db.Person().Iter().Where(person => person.Age >= age))
        {
            Runtime.Log($"{person.Name} has age {person.Age} >= {age}");
        }
    }
}
