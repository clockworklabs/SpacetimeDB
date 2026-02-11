using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using SpacetimeDB;

#pragma warning disable CA1050 // Declare types in namespaces - this is a test fixture, no need for a namespace.

[SpacetimeDB.Type]
public partial struct CustomStruct
{
    public const int IGNORE_ME = 0;
    public static readonly string IGNORE_ME_TOO = "";
    public int IntField;
    public string StringField;
}

[SpacetimeDB.Type]
public partial struct CustomClass
{
    public const int IGNORE_ME = 0;
    public static readonly string IGNORE_ME_TOO = "";
    public int IntField;
    public string StringField;
}

[StructLayout(LayoutKind.Auto)]
public partial struct CustomClass
{
    public int IgnoreExtraFields;
}

[SpacetimeDB.Type]
public enum CustomEnum
{
    EnumVariant1,
    EnumVariant2,
}

namespace System.Runtime.CompilerServices
{
    internal static class IsExternalInit { } // https://stackoverflow.com/a/64749403/1484415

    [AttributeUsage(AttributeTargets.Method, Inherited = false, AllowMultiple = false)]
    internal sealed class ModuleInitializerAttribute : Attribute { }
}

[SpacetimeDB.Type]
public partial record CustomTaggedEnum
    : SpacetimeDB.TaggedEnum<(int IntVariant, string StringVariant)>;

[SpacetimeDB.Type]
public partial struct PublicTable
{
    public int Id;
    public byte ByteField;
    public ushort UshortField;
    public uint UintField;
    public ulong UlongField;
    public U128 U128Field;
    public U256 U256Field;
    public sbyte SbyteField;
    public short ShortField;
    public int IntField;
    public long LongField;
    public I128 I128Field;
    public I256 I256Field;
    public bool BoolField;
    public float FloatField;
    public double DoubleField;
    public string StringField;
    public Identity IdentityField;
    public ConnectionId ConnectionIdField;
    public CustomStruct CustomStructField;
    public CustomClass CustomClassField;
    public CustomEnum CustomEnumField;
    public CustomTaggedEnum CustomTaggedEnumField;
    public List<int> ListField;
    public int? NullableValueField;
    public string? NullableReferenceField;
}

internal static class PublicTableViewRegressions
{
    [global::System.Runtime.CompilerServices.ModuleInitializer]
    internal static void Initialize()
    {
        ValidatePublicTableQuerySql();
        ValidatePublicTableViewSql();
        ValidateFindPublicTableByIdentitySql();
    }

    private sealed class PublicTableCols
    {
        public Col<PublicTable, int> Id { get; }

        public PublicTableCols(string tableName)
        {
            Id = new Col<PublicTable, int>(tableName, "Id");
        }
    }

    private sealed class PublicTableIxCols
    {
        public IxCol<PublicTable, int> Id { get; }

        public PublicTableIxCols(string tableName)
        {
            Id = new IxCol<PublicTable, int>(tableName, "Id");
        }
    }

    private static Table<PublicTable, PublicTableCols, PublicTableIxCols> MakeTable()
    {
        const string tableName = "PublicTable";
        return new Table<PublicTable, PublicTableCols, PublicTableIxCols>(
            tableName,
            new PublicTableCols(tableName),
            new PublicTableIxCols(tableName)
        );
    }

    private static string BuildPublicTableQuerySql() =>
        MakeTable().Where(cols => cols.Id.Eq(0)).Build().Sql;

    private static string BuildPublicTableViewSql()
    {
        var cols = new PublicTableCols("PublicTable");
        return MakeTable().Where(_ => cols.Id.Eq(0)).Build().Sql;
    }

    private static string BuildFindPublicTableByIdentitySql()
    {
        var table = MakeTable();
        return table.Where(cols => cols.Id.Eq(0)).Build().Sql;
    }

    /// <summary>
    /// Mirrors Module.PublicTableQuery server view to ensure the generated SQL stays in sync.
    /// </summary>
    [Conditional("DEBUG")]
    public static void ValidatePublicTableQuerySql()
    {
        var sql = BuildPublicTableQuerySql();
        Debug.Assert(
            sql == "SELECT * FROM \"PublicTable\" WHERE (\"PublicTable\".\"Id\" = 0)",
            $"Unexpected SQL produced for public_table_query: {sql}"
        );
    }

    /// <summary>
    /// Mirrors Module.PublicTableByIdentity (public_table_view) returning Option<PublicTable>.
    /// </summary>
    [Conditional("DEBUG")]
    public static void ValidatePublicTableViewSql()
    {
        var sql = BuildPublicTableViewSql();
        Debug.Assert(
            sql == "SELECT * FROM \"PublicTable\" WHERE (\"PublicTable\".\"Id\" = 0)",
            $"Unexpected SQL produced for public_table_view: {sql}"
        );
    }

    /// <summary>
    /// Mirrors Module.FindPublicTableByIdentity anonymous view.
    /// </summary>
    [Conditional("DEBUG")]
    public static void ValidateFindPublicTableByIdentitySql()
    {
        var sql = BuildFindPublicTableByIdentitySql();
        Debug.Assert(
            sql == "SELECT * FROM \"PublicTable\" WHERE (\"PublicTable\".\"Id\" = 0)",
            $"Unexpected SQL produced for find_public_table__by_identity: {sql}"
        );
    }
}
