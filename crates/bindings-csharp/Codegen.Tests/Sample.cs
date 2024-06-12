using System;
using System.Collections.Generic;
using SpacetimeDB.Module;
using static SpacetimeDB.Runtime;

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
    public int Id;

    public byte ByteField;
    public ushort UshortField;
    public uint UintField;
    public ulong UlongField;
    public UInt128 Uint128Field;
    public sbyte SbyteField;
    public short ShortField;
    public int IntField;
    public long LongField;
    public Int128 Int128Field;
    public bool BoolField;
    public float FloatField;
    public double DoubleField;
    public string StringField = "";
    public Identity IdentityField;
    public Address AddressField;
    public CustomStruct CustomStructField;
    public CustomClass CustomClassField;
    public CustomEnum CustomEnumField;
    public CustomTaggedEnum CustomTaggedEnumField = new CustomTaggedEnum.IntVariant(0);
    public List<int> ListField = [];
    public Dictionary<string, int> DictionaryField = [];
    public int? NullableValueField;
    public string? NullableReferenceField;
    public Dictionary<CustomEnum?, List<int?>?>? ComplexNestedField;
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
