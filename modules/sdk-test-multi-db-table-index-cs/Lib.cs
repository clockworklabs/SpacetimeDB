namespace SpacetimeDB.Sdk.Test.Multi;

using SpacetimeDB;

[Table(Public = true)]
partial struct User {
    [AutoInc]
    [PrimaryKey]
    public ulong Id;
    [Unique]
    public Identity Owner;
    public string Name;
}

[Table(Name = "MyTable1", Public = true, Index = "Name")]
[Table(Name = "MyTable2")]
partial struct MyTable {
    public string Name;

    [AutoInc]
    [PrimaryKey]
    public uint Foo;

    [Unique(Table = "MyTable2")]
    public uint Bar;
}

static class Module
{
    [Reducer]
    public static void AddUser(ReducerContext ctx, string name) {
        Runtime.Log($"Hello, {name}");

        ctx.Db.User().Insert(new() {
            Id = 0,
            Owner = ctx.Sender,
            Name = name,
        });
    }

    [Reducer]
    public static void GreetAllUsers(ReducerContext ctx)
    {
        Runtime.Log("Hello All");
        foreach (var user in ctx.Db.User().Iter())
        {
            Runtime.Log($"Hello, {user.Name}!");
        }
    }
}
