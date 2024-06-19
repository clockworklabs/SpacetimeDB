//HintName: PrivateTable.g.cs
#nullable enable

partial class PrivateTable : SpacetimeDB.BSATN.IStructuralReadWrite
{
    public void ReadFields(System.IO.BinaryReader reader) { }

    public void WriteFields(System.IO.BinaryWriter writer) { }

    public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<PrivateTable>
    {
        public PrivateTable Read(System.IO.BinaryReader reader) =>
            SpacetimeDB.BSATN.IStructuralReadWrite.Read<PrivateTable>(reader);

        public void Write(System.IO.BinaryWriter writer, PrivateTable value)
        {
            value.WriteFields(writer);
        }

        public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            registrar.RegisterType<PrivateTable>(
                typeRef => new SpacetimeDB.BSATN.AlgebraicType.Product(
                    new SpacetimeDB.BSATN.AggregateElement[] { }
                )
            );
    }

    private static readonly Lazy<SpacetimeDB.RawBindings.TableId> tableId =
        new(() => SpacetimeDB.Runtime.GetTableId(nameof(PrivateTable)));

    public static IEnumerable<PrivateTable> Iter() =>
        new SpacetimeDB.Runtime.RawTableIter(tableId.Value).Parse<PrivateTable>();

    public static SpacetimeDB.Module.TableDesc MakeTableDesc(
        SpacetimeDB.BSATN.ITypeRegistrar registrar
    ) =>
        new(
            new(nameof(PrivateTable), new SpacetimeDB.Module.ColumnDefWithAttrs[] { }, false),
            (SpacetimeDB.BSATN.AlgebraicType.Ref)new BSATN().GetAlgebraicType(registrar)
        );

    private static readonly Lazy<KeyValuePair<
        string,
        Action<BinaryWriter, object?>
    >[]> fieldTypeInfos = new(() => new KeyValuePair<string, Action<BinaryWriter, object?>>[] { });

    public static IEnumerable<PrivateTable> Query(
        System.Linq.Expressions.Expression<Func<PrivateTable, bool>> filter
    ) =>
        new SpacetimeDB.Runtime.RawTableIterFiltered(
            tableId.Value,
            SpacetimeDB.Filter.Filter.Compile<PrivateTable>(fieldTypeInfos.Value, filter)
        ).Parse<PrivateTable>();

    public void Insert()
    {
        var bytes = SpacetimeDB.Runtime.Insert(tableId.Value, this);
        // bytes should contain modified value now with autoinc fields updated
    }
} // PrivateTable
