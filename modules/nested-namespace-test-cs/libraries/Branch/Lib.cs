using System;
using SpacetimeDB;
using SpacetimeDB.Internal;

[assembly: Namespace(typeof(NestedLeaf.Functions), Accessor = "Leaf", Name = "nested_data")]

namespace NestedBranch;

// Deliberately no reducers or anonymous views: global dispatch must skip empty categories.
public static partial class Functions
{
#pragma warning disable STDB_UNSTABLE
    public static void ScheduleNext(HandlerContext ctx) =>
        NestedLeaf.Functions.VolatileNonatomicScheduleImmediateNext(ctx);
#pragma warning restore STDB_UNSTABLE

    public static void CheckSelection(
        ProcedureContext ctx,
        ProcedureContext caller,
        int expected
    ) => NestedLeaf.Functions.CheckProcedureSelection(ctx.As.Leaf, caller, expected);

    [View(Accessor = "QueryChild", Public = true)]
    public static IQuery<NestedLeaf.User> QueryChild(ViewContext ctx) =>
        ctx.From.Leaf.User().Where(row => row.Value.Eq(Module.GetInstanceId(ctx) + 1));

    [Procedure]
    public static int Instance(ProcedureContext ctx) =>
        ctx.WithTx(tx =>
        {
            if (tx.Db.Leaf.User.Id.Find(1)?.Value != Module.GetInstanceId(ctx) + 1)
            {
                throw new Exception("Branch accessed a different Leaf instance.");
            }
            return Module.GetInstanceId(ctx);
        });
}
