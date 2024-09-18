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

static partial class Module
{
    [Reducer]
    public static void AddUser(ReducerContext ctx, string name) {
        Runtime.Log($"Hello, {name}");

        var user = new User() {
            Id = ulong.MaxValue,
            Owner = ctx.Sender,
            Name = name,
        };
        ctx.Db.User.Insert(ref user);
    }

    [Reducer]
    public static void GreetAllUsers(ReducerContext ctx)
    {
        Runtime.Log("Hello All");
        foreach (var user in ctx.Db.User.Iter())
        {
            Runtime.Log($"Hello, {user.Name}!");
        }
    }
}