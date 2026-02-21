using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Placeholder")]
    public partial struct Placeholder
    {
        [PrimaryKey] public int Id;
    }

    [Reducer]
    public static void EmptyReducer_NoArgs(ReducerContext ctx) { }

    [Reducer]
    public static void EmptyReducer_WithInt(ReducerContext ctx, int count) { }

    [Reducer]
    public static void EmptyReducer_WithString(ReducerContext ctx, string name) { }

    [Reducer]
    public static void EmptyReducer_WithTwoArgs(ReducerContext ctx, int count, string name) { }

    [Reducer]
    public static void EmptyReducer_WithThreeArgs(ReducerContext ctx, bool active, float ratio, string label) { }
}