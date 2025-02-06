using System.Runtime.InteropServices;
using SpacetimeDB;

#pragma warning disable CA1050 // Declare types in namespaces - this is a test fixture, no need for a namespace.
#pragma warning disable STDB_UNSTABLE // Enable experimental SpacetimeDB features

[SpacetimeDB.Type]
public partial struct CustomStruct
{
    public const int IGNORE_ME = 0;
    public static readonly string IGNORE_ME_TOO = "";
    public int IntField;
    public string StringField;
}

[SpacetimeDB.Type]
public partial class CustomClass
{
    public const int IGNORE_ME = 0;
    public static readonly string IGNORE_ME_TOO = "";
    public int IntField = 0;
    public string StringField = "";
}

[StructLayout(LayoutKind.Auto)]
public partial class CustomClass
{
    public int IgnoreExtraFields;
}

[SpacetimeDB.Type]
public enum CustomEnum
{
    EnumVariant1,
    EnumVariant2,
}

[SpacetimeDB.Type]
public partial record CustomTaggedEnum
    : SpacetimeDB.TaggedEnum<(int IntVariant, string StringVariant)>;

[SpacetimeDB.Table]
public partial class PrivateTable { }

[SpacetimeDB.Table]
public partial struct PublicTable
{
    [SpacetimeDB.AutoInc]
    [SpacetimeDB.PrimaryKey]
    public int Id;

    public byte ByteField;
    public ushort UshortField;
    public uint UintField;
    public ulong UlongField;
    public UInt128 UInt128Field;
    public U128 U128Field;
    public U256 U256Field;
    public sbyte SbyteField;
    public short ShortField;
    public int IntField;
    public long LongField;
    public Int128 Int128Field;
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

public static partial class Reducers
{
    [SpacetimeDB.Reducer]
    public static void InsertData(ReducerContext ctx, PublicTable data)
    {
        ctx.Db.PublicTable.Insert(data);
        Log.Info("New list");
        foreach (var item in ctx.Db.PublicTable.Iter())
        {
            Log.Info($"Item: {item.StringField}");
        }
    }

    [SpacetimeDB.Reducer]
    public static void ScheduleImmediate(ReducerContext ctx, PublicTable data)
    {
        VolatileNonatomicScheduleImmediateInsertData(data);
    }
}

namespace Test
{
    namespace NestingNamespaces
    {
        public static partial class AndClasses
        {
            [SpacetimeDB.Reducer]
            public static void InsertData2(ReducerContext ctx, PublicTable data)
            {
                ctx.Db.PublicTable.Insert(data);
            }
        }
    }
}

public static partial class Timers
{
    [SpacetimeDB.Table(Scheduled = nameof(SendScheduledMessage))]
    public partial struct SendMessageTimer
    {
        [PrimaryKey]
        [AutoInc]
        public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
        public string Text;
    }

    [SpacetimeDB.Reducer]
    public static void SendScheduledMessage(ReducerContext ctx, SendMessageTimer arg)
    {
        // verify that fields were auto-added
        ulong id = arg.ScheduledId;
        SpacetimeDB.ScheduleAt scheduleAt = arg.ScheduledAt;
        string text = arg.Text;
    }

    [SpacetimeDB.Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        ctx.Db.SendMessageTimer.Insert(
            new SendMessageTimer
            {
                Text = "bot sending a message",
                ScheduledAt = ctx.Timestamp.AddSeconds(10),
            }
        );
    }
}

[SpacetimeDB.Table(Name = "MultiTable1", Public = true)]
[SpacetimeDB.Table(Name = "MultiTable2")]
public partial struct MultiTableRow
{
    [SpacetimeDB.Index.BTree(Table = "MultiTable1")]
    public string Name;

    [SpacetimeDB.AutoInc]
    [SpacetimeDB.PrimaryKey(Table = "MultiTable1")]
    public uint Foo;

    [SpacetimeDB.Unique(Table = "MultiTable2")]
    public uint Bar;

    [SpacetimeDB.Reducer]
    public static void InsertMultiData(ReducerContext ctx, MultiTableRow data)
    {
        // Verify that we have both tables generated on the context.
        ctx.Db.MultiTable1.Insert(data);
        ctx.Db.MultiTable2.Insert(data);
    }
}

[SpacetimeDB.Table]
[SpacetimeDB.Index.BTree(Name = "Location", Columns = ["X", "Y", "Z"])]
partial struct BTreeMultiColumn
{
    public uint X;
    public uint Y;
    public uint Z;
}

[SpacetimeDB.Table]
[SpacetimeDB.Index.BTree(Name = "Location", Columns = ["X", "Y"])]
partial struct BTreeViews
{
    [SpacetimeDB.PrimaryKey]
    public Identity Id;

    public uint X;
    public uint Y;

    [SpacetimeDB.Index.BTree]
    public string Faction;
}

[SpacetimeDB.Table]
partial struct RegressionMultipleUniqueIndexesHadSameName
{
    [SpacetimeDB.Unique]
    public uint Unique1;

    [SpacetimeDB.Unique]
    public uint Unique2;
}
