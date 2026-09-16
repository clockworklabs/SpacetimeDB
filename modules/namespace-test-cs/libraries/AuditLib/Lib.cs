using SpacetimeDB;

namespace AuditLib;

public class Marker { }

[Table(Accessor = "User", Public = true)]
public partial struct User
{
    [PrimaryKey]
    public uint Id;
    public string Message;
}

public static partial class Functions
{
    [Reducer]
    public static void Add(ReducerContext ctx, uint id) =>
        ctx.Db.User.Insert(new User { Id = id, Message = "audit" });

    [Procedure]
    public static ulong CountUsers(ProcedureContext ctx) => ctx.WithTx(tx => tx.Db.User.Count);
}
