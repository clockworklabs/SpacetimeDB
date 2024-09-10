﻿//HintName: PrivateTable.cs
// <auto-generated />
#nullable enable

partial class PrivateTable : SpacetimeDB.Internal.ITable<PrivateTable>
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
            registrar.RegisterType<PrivateTable>(_ => new SpacetimeDB.BSATN.AlgebraicType.Product(
                new SpacetimeDB.BSATN.AggregateElement[] { }
            ));
    }

    void SpacetimeDB.Internal.ITable<PrivateTable>.ReadGenFields(System.IO.BinaryReader reader) { }

    static SpacetimeDB.Internal.Module.TableDesc SpacetimeDB.Internal.ITable<PrivateTable>.MakeTableDesc(
        SpacetimeDB.BSATN.ITypeRegistrar registrar
    ) =>
        new(
            new(
                nameof(PrivateTable),
                new SpacetimeDB.Internal.Module.ColumnDefWithAttrs[] { },
                false,
                null
            ),
            (SpacetimeDB.BSATN.AlgebraicType.Ref)new BSATN().GetAlgebraicType(registrar)
        );

    static SpacetimeDB.Internal.Filter SpacetimeDB.Internal.ITable<PrivateTable>.CreateFilter() =>
        new([]);

    public static IEnumerable<PrivateTable> Iter() =>
        SpacetimeDB.Internal.ITable<PrivateTable>.Iter();

    public static IEnumerable<PrivateTable> Query(
        System.Linq.Expressions.Expression<Func<PrivateTable, bool>> predicate
    ) => SpacetimeDB.Internal.ITable<PrivateTable>.Query(predicate);

    public void Insert() => SpacetimeDB.Internal.ITable<PrivateTable>.Insert(this);
} // PrivateTable
