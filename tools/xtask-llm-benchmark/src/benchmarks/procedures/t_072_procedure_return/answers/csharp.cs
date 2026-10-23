using SpacetimeDB;
#pragma warning disable STDB_UNSTABLE

[SpacetimeDB.Type]
public partial struct Summary { public uint Total; public string Label; }

public static partial class Module
{
    [SpacetimeDB.Procedure]
    public static Summary CalculateSummary(ProcedureContext ctx, uint lhs, uint rhs) =>
        new() { Total = lhs + rhs, Label = "calculated" };
}
