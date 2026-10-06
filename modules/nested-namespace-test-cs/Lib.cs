using System;
using SpacetimeDB;
using SpacetimeDB.Internal;

[assembly: Namespace(typeof(NestedBranch.Functions), Accessor = "Branch", Name = "branch_data")]
[assembly: Namespace(typeof(NestedLeaf.Functions), Accessor = "Leaf", Name = "leaf_data")]
[assembly: Namespace(typeof(NestedLeaf.Functions), Accessor = "SecondLeaf", Name = "second_data")]
[assembly: Namespace(typeof(NestedOuter.Marker), Accessor = "class", Name = "outer_data")]

namespace NestedRoot;

[Table(Accessor = "User", Public = true)]
public partial struct User
{
    public string Name;
}

[Type]
public partial struct NestedResult
{
    public int Value;
}

public static partial class Functions
{
    [Reducer]
    public static void Ping(ReducerContext ctx) => Log.Info($"root:{Module.GetInstanceId(ctx)}");

    [Procedure]
    public static int Instance(ProcedureContext ctx) => Module.GetInstanceId(ctx);

    [Reducer]
    public static void CheckTables(ReducerContext ctx)
    {
        if (
            ctx.Db.User.Count != 0
            || ctx.Db.Branch.Leaf.User.Id.Find(1)?.Value != 2
            || ctx.Db.Leaf.User.Id.Find(1)?.Value != 3
            || ctx.Db.Promoted.User.Id.Find(1)?.Value != 4
            || ctx.Db.SecondLeaf.User.Id.Find(1)?.Value != 5
            || ctx.Db.@class.User.Id.Find(1)?.Value != 6
            || ctx.Db.@class.Branch.Leaf.User.Id.Find(1)?.Value != 8
        )
        {
            throw new Exception("Root accessors did not select separate table instances.");
        }
        var nested = ctx.Db.@class.Branch.Leaf.User;
        nested.Id.Update(new NestedLeaf.User { Id = 1, Value = 80 });
        if (ctx.Db.Branch.Leaf.User.Id.Find(1)?.Value != 2)
        {
            throw new Exception("Deep accessor modified the other Branch instance.");
        }
        nested.Id.Update(new NestedLeaf.User { Id = 1, Value = 8 });
        // Without selecting a mount, the root cannot choose between four Leaf instances.
        try
        {
            NestedLeaf.Functions.Count(ctx);
            throw new Exception("Ambiguous library access unexpectedly succeeded.");
        }
        catch (InvalidOperationException error) when (error.Message.Contains("multiple instances"))
        { }
    }

    [View(Accessor = "Nested", Public = true)]
    public static NestedResult? Nested(ViewContext ctx) =>
        new NestedResult { Value = ctx.Db.Branch.Leaf.User.Id.Find(1)!.Value.Value };

    [View(Accessor = "Deep", Public = true)]
    public static NestedResult? Deep(AnonymousViewContext ctx)
    {
        if (ctx.Db.@class.User.Id.Find(1)?.Value != 6)
        {
            throw new Exception("Public contribution was not exposed in the read-only namespace.");
        }
        return new NestedResult { Value = ctx.Db.@class.Branch.Leaf.User.Id.Find(1)!.Value.Value };
    }
}
