//HintName: Test.NestingNamespaces.AndClasses.g.cs
#nullable enable

namespace Test.NestingNamespaces
{
    partial class AndClasses
    {
        // We need to generate a class, but we don't want it to be visible to users in autocomplete.
        [System.ComponentModel.EditorBrowsable(System.ComponentModel.EditorBrowsableState.Advanced)]
        internal class InsertData2_BSATN : IReducer
        {
            internal static readonly PublicTable.BSATN data = new();

            SpacetimeDB.Module.ReducerDef IReducer.MakeReducerDef(
                SpacetimeDB.BSATN.ITypeRegistrar registrar
            )
            {
                return new(
                    "test_custom_name_and_reducer_ctx",
                    new SpacetimeDB.BSATN.AggregateElement(
                        nameof(data),
                        data.GetAlgebraicType(registrar)
                    )
                );
            }

            void IReducer.Invoke(BinaryReader reader, SpacetimeDB.Runtime.ReducerContext ctx)
            {
                InsertData2(ctx, data.Read(reader));
            }
        }

        public static SpacetimeDB.Runtime.ScheduleToken ScheduleInsertData2(
            DateTimeOffset time,
            PublicTable data
        )
        {
            using var stream = new MemoryStream();
            using var writer = new BinaryWriter(stream);
            InsertData2_BSATN.data.Write(writer, data);
            return new(nameof(InsertData2), stream, time);
        }
    } // AndClasses
} // namespace
