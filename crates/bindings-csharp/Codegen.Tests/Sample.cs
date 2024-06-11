using System;
using System.Collections.Generic;
using SpacetimeDB.Module;
using static SpacetimeDB.Runtime;

[SpacetimeDB.Type]
public partial struct CustomStruct
{
    public const int IGNORE_ME = 0;
    public static readonly string IGNORE_ME_TOO = "";
    public int intField;
    public string stringField;
}

[SpacetimeDB.Type]
public partial struct CustomClass
{
    public const int IGNORE_ME = 0;
    public static readonly string IGNORE_ME_TOO = "";
    public int intField;
    public string stringField;
}

[SpacetimeDB.Type]
public enum CustomEnum
{
    EnumVariant1,
    EnumVariant2
}

[SpacetimeDB.Type]
public partial record CustomTaggedEnum
    : SpacetimeDB.TaggedEnum<(int IntVariant, string StringVariant)>;

[SpacetimeDB.Table]
public partial class PrivateTable { }

[SpacetimeDB.Table]
public partial class PublicTable
{
    [SpacetimeDB.Column(ColumnAttrs.PrimaryKeyAuto)]
    public int id;

    public byte byteField;
    public ushort ushortField;
    public uint uintField;
    public ulong ulongField;
    public UInt128 uint128Field;
    public sbyte sbyteField;
    public short shortField;
    public int intField;
    public long longField;
    public Int128 int128Field;
    public bool boolField;
    public float floatField;
    public double doubleField;
    public string stringField = "";
    public Identity identityField;
    public Address addressField;
    public CustomStruct customStructField;
    public CustomClass customClassField;
    public CustomEnum customEnumField;
    public CustomTaggedEnum customTaggedEnumField = new CustomTaggedEnum.IntVariant(0);
    public List<int> listField = [];
    public Dictionary<string, int> dictionaryField = [];
    public int? nullableValueField;
    public string? nullableReferenceField;
}

public static class Reducers
{
    [SpacetimeDB.Reducer]
    static void InsertData(PublicTable data)
    {
        data.Insert();
    }
}

namespace Test
{
    namespace NestingNamespaces
    {
        public static class AndClasses
        {
            [SpacetimeDB.Reducer("test_custom_name_and_reducer_ctx")]
            public static void InsertData2(ReducerContext ctx, PublicTable data)
            {
                data.Insert();
            }
        }
    }
}
