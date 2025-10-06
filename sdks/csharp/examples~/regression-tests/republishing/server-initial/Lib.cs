using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Name = "ExampleData", Public = true)]
    public partial struct ExampleData
    {
        [SpacetimeDB.PrimaryKey]
        public uint Primary;
        public uint TestPass;
    }

    [SpacetimeDB.Reducer]
    public static void Insert(ReducerContext ctx, uint id)
    {
        var exampleData = ctx.Db.ExampleData.Insert(new ExampleData { Primary = id, TestPass = 1 });
        Log.Info($"Inserted key {exampleData.Primary} on pass {exampleData.TestPass}");
    }
}
