#pragma warning disable STDB_UNSTABLE
namespace SpacetimeDB.Modules.ModuleTestCs;

using SpacetimeDB;

[SpacetimeDB.Env]
public partial struct ModuleEnvironmentSchema
{
    public string? MISSING;
    public string? EMPTY;
    public string? UTF8;
    public string? NUL;
    public string? MAXIMUM;
}

public static partial class EnvironmentTests
{
    [Reducer]
    public static void expect_environment(ReducerContext ctx, string key, string? expected)
    {
        if (ctx.Env.EMPTY != ctx.Env.Get("EMPTY")) throw new Exception("named environment mismatch");
        if (ctx.Env.Get(key) != expected) throw new Exception("environment value mismatch");
    }

    [Procedure]
    public static string? read_environment(ProcedureContext ctx, string key)
    {
        var outside = ctx.Env.Get(key);
        ctx.WithTx(tx =>
        {
            if (tx.Env.Get(key) != outside) throw new Exception("transaction environment value mismatch");
            return true;
        });
        return outside;
    }
}
