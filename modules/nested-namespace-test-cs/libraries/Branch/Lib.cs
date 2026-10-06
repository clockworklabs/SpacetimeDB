using System;
using SpacetimeDB;
using SpacetimeDB.Internal;

[assembly: Namespace(typeof(NestedLeaf.Functions), Accessor = "Leaf", Name = "nested_data")]

namespace NestedBranch;

// Deliberately no reducers or views: global dispatch must skip empty categories.
public static partial class Functions
{
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
