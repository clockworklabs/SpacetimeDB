using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Name = "person", Public = true)]
    public partial struct Person
    {
        public string Name;
    }

    [SpacetimeDB.Reducer]
    public static void Add(ReducerContext ctx, string name)
    {
        ctx.Db.person.Insert(new Person { Name = name });
    }

    [SpacetimeDB.Reducer]
    public static void SayHello(ReducerContext ctx)
    {
        foreach (var person in ctx.Db.person.Iter())
        {
            Log.Info($"Hello, {person.Name}!");
        }
        Log.Info("Hello, World!");
    }
