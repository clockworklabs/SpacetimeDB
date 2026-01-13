using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Name = "person", Public = true)]
    public partial struct Person
    {
        public string Name;
    }

    [SpacetimeDB.Reducer(Name = "add")]
    public static void Add(ReducerContext ctx, string name)
    {
        ctx.Db.Person.Insert(new Person { Name = name });
    }

    [SpacetimeDB.Reducer(Name = "say_hello")]
    public static void SayHello(ReducerContext ctx)
    {
        foreach (var person in ctx.Db.Person.Iter())
        {
            Log.Info($"Hello, {person.Name}!");
        }
        Log.Info("Hello, World!");
    }
}
