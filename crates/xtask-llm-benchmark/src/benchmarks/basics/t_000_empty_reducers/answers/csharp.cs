using SpacetimeDB;

public static partial class Module
{
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