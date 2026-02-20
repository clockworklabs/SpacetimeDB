using SpacetimeDB;

#pragma warning disable CA1050 // Declare types in namespaces - this is a test fixture, no need for a namespace.

public static partial class Module
{
    [SpacetimeDB.Settings]
    public const SpacetimeDB.Internal.CaseConversionPolicy CASE_CONVERSION_POLICY =
        SpacetimeDB.Internal.CaseConversionPolicy.SnakeCase;
}

[SpacetimeDB.Type]
public partial struct DemoType
{
    public int A;
}

[SpacetimeDB.Table(Accessor = "DemoTable", Name = "canonical_table", Public = true)]
public partial struct DemoTable
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.Index.BTree(Accessor = "ById", Name = "canonical_index")]
    public int Id;

    public int Value;
}

public static partial class Reducers
{
    [SpacetimeDB.Reducer(Name = "canonical_reducer")]
    public static void DemoReducer(ReducerContext ctx, int value)
    {
        ctx.Db.DemoTable.Insert(new DemoTable { Id = value, Value = value });
    }

    [SpacetimeDB.Procedure(Name = "canonical_procedure")]
    public static void DemoProcedure(ProcedureContext ctx) { }

    [SpacetimeDB.View(Accessor = "demo_view", Name = "canonical_view", Public = true)]
    public static List<DemoTable> DemoView(ViewContext ctx)
    {
        return new List<DemoTable>();
    }
}
