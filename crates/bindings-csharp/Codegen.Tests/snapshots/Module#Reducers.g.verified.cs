//HintName: Reducers.g.cs
#nullable enable

partial class Reducers
{
    // We need to generate a class, but we don't want it to be visible to users in autocomplete.
    [System.ComponentModel.EditorBrowsable(System.ComponentModel.EditorBrowsableState.Advanced)]
    internal class InsertData_BSATN : IReducer
    {
        internal static readonly PublicTable.BSATN data = new();

        SpacetimeDB.Module.ReducerDef IReducer.MakeReducerDef(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        )
        {
            return new(
                "InsertData",
                new SpacetimeDB.BSATN.AggregateElement(
                    nameof(data),
                    data.GetAlgebraicType(registrar)
                )
            );
        }

        void IReducer.Invoke(BinaryReader reader, SpacetimeDB.Runtime.ReducerContext ctx)
        {
            InsertData(data.Read(reader));
        }
    }

    public static SpacetimeDB.Runtime.ScheduleToken ScheduleInsertData(
        DateTimeOffset time,
        PublicTable data
    )
    {
        using var stream = new MemoryStream();
        using var writer = new BinaryWriter(stream);
        InsertData_BSATN.data.Write(writer, data);
        return new(nameof(InsertData), stream, time);
    }
} // Reducers
