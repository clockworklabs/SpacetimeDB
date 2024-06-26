// Standard implicit usings.
global using global::System;
global using global::System.Collections.Generic;
global using global::System.IO;
global using global::System.Linq;
// Our own code.
using SpacetimeDB;

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
public partial struct PublicTable
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
    public string StringField;
    public Identity IdentityField;
    public Address AddressField;
    public CustomStruct CustomStructField;
    public CustomClass CustomClassField;
    public CustomEnum CustomEnumField;
    public CustomTaggedEnum CustomTaggedEnumField;
    public List<int> ListField;
    public Dictionary<string, int> DictionaryField;
    public int? NullableValueField;
    public string? NullableReferenceField;
    public Dictionary<CustomEnum, List<int?>?>? ComplexNestedField;
}

public static partial class Reducers
{
    [SpacetimeDB.Reducer]
    public static void InsertData(PublicTable data)
    {
        data.Insert();
        Runtime.Log("New list");
        foreach (var item in PublicTable.Iter())
        {
            Runtime.Log($"Item: {item.StringField}");
        }
    }
}

namespace Test
{
    namespace NestingNamespaces
    {
        public static partial class AndClasses
        {
            [SpacetimeDB.Reducer("test_custom_name_and_reducer_ctx")]
            public static void InsertData2(ReducerContext ctx, PublicTable data)
            {
                data.Insert();
            }
        }
    }
}

public static partial class Timers
{
    [SpacetimeDB.Table(Scheduled = nameof(SendScheduledMessage))]
    public partial struct SendMessageTimer
    {
        public string Text;
    }

    [SpacetimeDB.Reducer]
    public static void SendScheduledMessage(SendMessageTimer arg)
    {
        // verify that fields were auto-added
        ulong id = arg.ScheduledId;
        SpacetimeDB.ScheduleAt scheduleAt = arg.ScheduledAt;
        string text = arg.Text;
    }

    [SpacetimeDB.Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        new SendMessageTimer
        {
            Text = "bot sending a message",
            ScheduledAt = new ScheduleAt.Time(ctx.Time.AddSeconds(10))
        }.Insert();
    }
}
