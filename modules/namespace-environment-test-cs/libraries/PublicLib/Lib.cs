using SpacetimeDB;

namespace PublicLib;

[SpacetimeDB.Env]
public struct EnvironmentSchema
{
    public string? PUBLIC_LIBRARY_TEST;
}

public static partial class Functions
{
    [Procedure]
    public static string ReadPublicEnvironment(ProcedureContext ctx)
    {
        var value = ctx.Env.PUBLIC_LIBRARY_TEST;
        if (ctx.WithTx(tx => tx.Env.PUBLIC_LIBRARY_TEST) != value)
        {
            throw new Exception("Public library transaction environment mismatch");
        }

        return value ?? "unset";
    }
}
