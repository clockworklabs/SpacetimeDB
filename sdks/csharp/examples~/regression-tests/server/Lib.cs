// Server module for regression tests.
// Everything we're testing for happens SDK-side so this module is very uninteresting.

using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Name = "ExampleData", Public = true)]
    public partial struct ExampleData
    {
        [SpacetimeDB.PrimaryKey]
        public uint Id;

        [SpacetimeDB.Index.BTree]
        public uint Indexed;
    }

    [SpacetimeDB.View(Name = "GetExampleDataById", Public = true)]
    public static ExampleData? GetExampleDataById(ViewContext ctx) //, uint id)
    {
        return ctx.Db.ExampleData.Id.Find(0);
    }

    [SpacetimeDB.View(Name = "GetAnonymousExampleDataById", Public = true)]
    public static ExampleData? GetAnonymousExampleDataById(AnonymousViewContext ctx) //, uint id)
    {
        return ctx.Db.ExampleData.Id.Find(0);
    }

    [SpacetimeDB.Reducer]
    public static void Delete(ReducerContext ctx, uint id)
    {
        ctx.Db.ExampleData.Id.Delete(id);
    }

    [SpacetimeDB.Reducer]
    public static void Add(ReducerContext ctx, uint id, uint indexed)
    {
        ctx.Db.ExampleData.Insert(new ExampleData { Id = id, Indexed = indexed });
    }

    [SpacetimeDB.Reducer]
    public static void ThrowError(ReducerContext ctx, string error)
    {
        throw new Exception(error);
    }
}
