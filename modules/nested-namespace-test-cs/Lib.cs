using System;
using SpacetimeDB;
using SpacetimeDB.Internal;

#pragma warning disable STDB_UNSTABLE

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
    [View(Accessor = "QueryDeep", Public = true)]
    public static IQuery<NestedLeaf.RefUser> QueryDeep(ViewContext ctx) =>
        ctx
            .From.@class.Branch.Leaf.RefUser()
            .Where(row => row.Value.Eq(8))
            .LeftSemijoin(ctx.From.SecondLeaf.RefUser(), (deep, other) => deep.Id.Eq(other.Id));

    [View(Accessor = "QueryPublic", Public = true)]
    public static IQuery<NestedLeaf.RefUser> QueryPublic(AnonymousViewContext ctx) =>
        ctx
            .From.Promoted.RefUser()
            .Where(row => row.Value.Eq(4))
            .RightSemijoin(ctx.From.@class.RefUser(), (other, direct) => other.Id.Eq(direct.Id));

    [Reducer]
    public static void Ping(ReducerContext ctx) => Log.Info($"root:{Module.GetInstanceId(ctx)}");

    [Procedure]
    public static int Instance(ProcedureContext ctx)
    {
        NestedBranch.Functions.CheckSelection(ctx.As.Branch, ctx, 2);
        NestedBranch.Functions.CheckSelection(ctx.As.@class.Branch, ctx, 8);
        ProcedureTxContext? selected = null;
        ctx.WithTx(tx =>
        {
            selected = tx.As.@class.Branch.Leaf;
            NestedLeaf.Functions.BeginSelectedTx(selected);
            return true;
        });
        try
        {
            ctx.WithTx<bool>(tx =>
            {
                NestedLeaf.Functions.Write(selected!, -1);
                tx.Db.User.Insert(new User { Name = "rollback" });
                throw new InvalidOperationException("rollback");
            });
        }
        catch (InvalidOperationException error) when (error.Message == "rollback") { }
        ctx.WithTx(tx =>
        {
            NestedLeaf.Functions.FinishSelectedTx(selected!, tx);
            if (tx.Db.User.Count != 0 || tx.Db.Branch.Leaf.User.Count != 0)
            {
                throw new Exception("Selected transaction changed another scope.");
            }
            return true;
        });
        return Module.GetInstanceId(ctx);
    }

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
        ReducerContext[] selected =
        [
            ctx.As.Branch.Leaf,
            ctx.As.Leaf,
            ctx.As.Promoted,
            ctx.As.SecondLeaf,
            ctx.As.@class,
            ctx.As.@class.Branch.Leaf,
        ];
        int[] instanceIds = [2, 3, 4, 5, 6, 8];
        var previous = ctx.NewUuidV7();
        for (var i = 0; i < selected.Length; i++)
        {
            var next = NestedLeaf.Functions.CheckReducerSelection(selected[i], ctx, instanceIds[i]);
            if (next.GetCounter() != previous.GetCounter() + 1)
            {
                throw new Exception("Selected reducer copied its UUID counter.");
            }
            previous = next;
        }
        if (
            ctx.NewUuidV7().GetCounter() != previous.GetCounter() + 1
            || Module.GetInstanceId(ctx) != 0
            || ctx.Db.User.Count != 0
        )
        {
            throw new Exception("Context selection changed the caller or copied its UUID counter.");
        }
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
    public static NestedResult? Nested(ViewContext ctx)
    {
        ViewContext selected = ctx.As.Branch.Leaf;
        if (selected.Sender != ctx.Sender || Module.GetInstanceId(ctx) != 0)
        {
            throw new Exception("View selection changed caller state.");
        }
        return new NestedResult { Value = NestedLeaf.Functions.Current(selected)!.Value.Value };
    }

    [View(Accessor = "Deep", Public = true)]
    public static NestedResult? Deep(AnonymousViewContext ctx)
    {
        if (ctx.Db.@class.User.Id.Find(1)?.Value != 6)
        {
            throw new Exception("Public contribution was not exposed in the read-only namespace.");
        }
        return new NestedResult { Value = NestedLeaf.Functions.Read(ctx.As.@class.Branch.Leaf) };
    }

    [HttpHandler]
    public static HttpResponse CheckContextSelection(HandlerContext ctx, HttpRequest request)
    {
        NestedLeaf.Functions.CheckHandlerSelection(ctx.As.Branch.Leaf, ctx);
        NestedLeaf.Functions.CheckHandlerSelection(ctx.As.@class.Branch.Leaf, ctx);
        ctx.WithTx(tx =>
        {
            NestedLeaf.Functions.CheckSelectedHandlerTx(tx.As.SecondLeaf, tx);
            return true;
        });
        return new(200, HttpVersion.Http11, [], HttpBody.FromString("selected"));
    }

    [HttpRouter]
    public static Router Routes() => Router.New().Get("/contexts", Handlers.CheckContextSelection);
}
