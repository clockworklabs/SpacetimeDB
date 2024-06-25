//HintName: Problematic.g.cs
#nullable enable

namespace Problematic
{
    // We need to generate a class, but we don't want it to be visible to users in autocomplete.
    [System.ComponentModel.EditorBrowsable(System.ComponentModel.EditorBrowsableState.Advanced)]
    internal class ReducerWithNonVoidReturnType_BSATN : IReducer
    {
        SpacetimeDB.Module.ReducerDef IReducer.MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        )
        {
            return new("ReducerWithNonVoidReturnType");
        }

        void IReducer.Invoke(BinaryReader reader, SpacetimeDB.Runtime.ReducerContext ctx)
        {
            ReducerWithNonVoidReturnType();
        }
    }

    public static SpacetimeDB.Runtime.ScheduleToken ScheduleReducerWithNonVoidReturnType(
        DateTimeOffset time
    )
    {
        using var stream = new MemoryStream();
        using var writer = new BinaryWriter(stream);

        return new(nameof(ReducerWithNonVoidReturnType), stream, time);
    }
} // namespace
