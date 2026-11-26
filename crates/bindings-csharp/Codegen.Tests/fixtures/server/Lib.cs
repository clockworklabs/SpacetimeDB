using System.Runtime.InteropServices;
using SpacetimeDB;

#pragma warning disable CA1050 // Declare types in namespaces - this is a test fixture, no need for a namespace.
#pragma warning disable STDB_UNSTABLE // Enable experimental SpacetimeDB features

[SpacetimeDB.Env]
public struct EnvironmentSchema
{
    public string REQUIRED;
    public string? OPTIONAL;

    [SpacetimeDB.EnvValues("dev", "prod")]
    public string MODE;
}

[SpacetimeDB.Type]
public partial struct CustomStruct
{
    public const int IGNORE_ME = 0;
    public static readonly string IGNORE_ME_TOO = "";
    public int IntField;
    public string StringField;
    public int? NullableIntField;
    public string? NullableStringField;
}

[SpacetimeDB.Type]
public partial class CustomClass
{
    public const int IGNORE_ME = 0;
    public static readonly string IGNORE_ME_TOO = "";
    public int IntField = 0;
    public string StringField = "";
    public int? NullableIntField;
    public string? NullableStringField;
}

[SpacetimeDB.Type]
public partial class CustomRecord
{
    public const int IGNORE_ME = 0;
    public static readonly string IGNORE_ME_TOO = "";
    public int IntField = 0;
    public string StringField = "";
    public int? NullableIntField;
    public string? NullableStringField;
}

[StructLayout(LayoutKind.Auto)]
public partial class CustomClass
{
    public int IgnoreExtraFields;
}

[SpacetimeDB.Type]
public partial class CustomNestedClass
{
    public CustomClass NestedClass = new();
    public CustomClass? NestedNullableClass = null;
    public CustomEnum NestedEnum = CustomEnum.EnumVariant1;
    public CustomEnum? NestedNullableEnum = null;
    public CustomTaggedEnum NestedTaggedEnum = new CustomTaggedEnum.NullableIntVariant(null);
    public CustomTaggedEnum? NestedNullableTaggedEnum = null;
    public CustomRecord NestedCustomRecord = new();
    public CustomRecord? NestedNullableCustomRecord = null;
}

[SpacetimeDB.Type]
public partial class ContainsNestedLists
{
    public List<int> IntList = [];
    public List<string> StringList = [];
    public int[] IntArray = [];
    public string[] StringArray = [];
    public List<int[][]> IntArrayArrayList = [];
    public List<List<int>>[] IntListListArray = [];
    public List<string[][]> StringArrayArrayList = [];
    public List<List<string>>[] StringListListArray = [];
}

[SpacetimeDB.Type]
public partial class EmptyClass { }

[SpacetimeDB.Type]
public partial struct EmptyStruct { }

[SpacetimeDB.Type]
public partial record EmptyRecord { }

[SpacetimeDB.Type]
public enum CustomEnum
{
    EnumVariant1,
    EnumVariant2,
}

[SpacetimeDB.Type]
public partial record CustomTaggedEnum
    : SpacetimeDB.TaggedEnum<(
        int IntVariant,
        string StringVariant,
        int? NullableIntVariant,
        string? NullableStringVariant
    )>;

[SpacetimeDB.Type(Name = "game_world.NamedStruct")]
public partial struct NamedStruct
{
    public int IntField;
}

[SpacetimeDB.Type(Name = "game_world.NamedTaggedEnum")]
public partial record NamedTaggedEnum
    : SpacetimeDB.TaggedEnum<(int IntVariant, string StringVariant)>;

[SpacetimeDB.Table(Event = true)]
public partial class PrivateTable { }

[SpacetimeDB.Table(Public = true)]
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
                ScheduledAt = ctx.Timestamp + new TimeDuration(10_000_000),
            }
        );
    }
}

[SpacetimeDB.Table(
    Accessor = "MultiTable1",
    Public = true,
    CachePolicy = SpacetimeDB.TableCachePolicy.RequiredOnDisk
)]
[SpacetimeDB.Table(
    Accessor = "MultiTable2",
    CachePolicy = SpacetimeDB.TableCachePolicy.NoRequirement
)]
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
[SpacetimeDB.Index.BTree(Accessor = "Location", Columns = ["X", "Y", "Z"])]
partial struct BTreeMultiColumn
{
    public uint X;
    public uint Y;
    public uint Z;
}

[SpacetimeDB.Table]
[SpacetimeDB.Index.BTree(Accessor = "Location", Columns = ["X", "Y"])]
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

/// <summary>
/// These used to cause conflicts when generating the BSATN struct for a type.
/// </summary>
[SpacetimeDB.Type]
partial struct FormerlyForbiddenFieldNames
{
    public uint Read;
    public uint Write;
    public uint GetAlgebraicType;
}

public class Module
{
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter ALL_PUBLIC_TABLES = new Filter.Sql("SELECT * FROM PublicTable");

    [SpacetimeDB.View(Accessor = "public_table_view", Public = true)]
    public static PublicTable? PublicTableByIdentity(ViewContext ctx)
    {
        return (PublicTable?)ctx.Db.PublicTable.Id.Find(0);
    }

    [SpacetimeDB.View(Accessor = "public_table_query", Public = true)]
    public static IQuery<PublicTable> PublicTableQuery(ViewContext ctx)
    {
        return ctx.From.PublicTable().Where(cols => cols.Id.Eq(0));
    }

    [SpacetimeDB.View(Accessor = "find_public_table__by_identity", Public = true)]
    public static PublicTable? FindPublicTableByIdentity(AnonymousViewContext ctx)
    {
        return (PublicTable?)ctx.Db.PublicTable.Id.Find(0);
    }
}

/// <summary>
/// Receiver bindings as `spacetime generate --bindings module --lang csharp` writes them.
/// </summary>
namespace RemoteBindings.Receiver
{
    public readonly record struct Identity(global::SpacetimeDB.Identity Value)
    {
        public static explicit operator Identity(global::SpacetimeDB.Identity value) => new(value);

        public static implicit operator global::SpacetimeDB.Identity(Identity value) => value.Value;

        public readonly struct BSATN : global::SpacetimeDB.BSATN.IReadWrite<Identity>
        {
            public Identity Read(global::System.IO.BinaryReader reader) =>
                new(new global::SpacetimeDB.Identity.BSATN().Read(reader));

            public void Write(global::System.IO.BinaryWriter writer, Identity value) =>
                new global::SpacetimeDB.Identity.BSATN().Write(writer, value.Value);

            public global::SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(
                global::SpacetimeDB.BSATN.ITypeRegistrar registrar
            ) => new global::SpacetimeDB.Identity.BSATN().GetAlgebraicType(registrar);
        }
    }

    [global::SpacetimeDB.Type(Name = "receiver.Details")]
    public sealed partial class Details
    {
        public string Label = "";
    }

    public static partial class Reducers
    {
        public abstract class AdmitPlayer : global::SpacetimeDB.Internal.IOutboxReceiver
        {
            public const string ReducerName = "admit_player";
            public const string SignatureHash =
                "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

            public string @name = default!;
            public global::RemoteBindings.Receiver.Details @player_details = default!;
            public string? @nickname = default!;
        }

        public abstract class Ping : global::SpacetimeDB.Internal.IOutboxReceiver
        {
            public const string ReducerName = "ping";
            public const string SignatureHash =
                "ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100";

            public ulong @seq = default!;
        }
    }
}

[SpacetimeDB.Table(
    Accessor = "AdmitOutbox",
    Outbox = typeof(RemoteBindings.Receiver.Reducers.AdmitPlayer)
)]
[SpacetimeDB.Index.BTree(Accessor = "ByNameAndExtra", Columns = ["Name", "Extra"])]
public partial struct AdmitOutboxRow
{
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    public ulong MsgId;

    [SpacetimeDB.Target]
    public RemoteBindings.Receiver.Identity Target;

    // Matches `name` by its snake_case name.
    public string Name;

    [SpacetimeDB.Param("player_details")]
    public RemoteBindings.Receiver.Details Info;

    public string? Nickname;

    // Columns the reducer doesn't take are allowed.
    public uint Extra;
}

// One row type with an outbox accessor and a normal one.
[SpacetimeDB.Table(Accessor = "PingOutbox", Outbox = typeof(RemoteBindings.Receiver.Reducers.Ping))]
[SpacetimeDB.Table(Accessor = "PingLog")]
public partial struct PingRow
{
    // The message ID can also be a parameter.
    [SpacetimeDB.PrimaryKey]
    [SpacetimeDB.AutoInc]
    [SpacetimeDB.Param("seq", Table = "PingOutbox")]
    public ulong Id;

    [SpacetimeDB.Target(Table = "PingOutbox")]
    public RemoteBindings.Receiver.Identity Destination;

    [SpacetimeDB.Unique]
    public string Tag;
}

// [Outbox] declares MsgId, Target, Name, PlayerDetails and Nickname.
[SpacetimeDB.Outbox(
    typeof(RemoteBindings.Receiver.Reducers.AdmitPlayer),
    Accessor = "AdmitGenerated"
)]
public partial struct AdmitGeneratedRow { }

public static partial class OutboxReducers
{
    [SpacetimeDB.Reducer]
    public static void SendOutboxes(ReducerContext ctx)
    {
        var target = (RemoteBindings.Receiver.Identity)ctx.Sender;
        ctx.Db.AdmitOutbox.Insert(
            new AdmitOutboxRow
            {
                Target = target,
                Name = "player",
                Info = new RemoteBindings.Receiver.Details(),
            }
        );
        foreach (var row in ctx.Db.AdmitOutbox.ByNameAndExtra.Filter(("player", 0u))) { }
        ctx.Db.PingOutbox.Insert(new PingRow { Destination = target, Tag = "ping" });
        _ = ctx.Db.PingOutbox.Tag.Find("ping");
        ctx.Db.PingLog.Delete(new PingRow { Tag = "log" });
    }

    // [OnResult] reducers may use the `On` prefix.
    [SpacetimeDB.Reducer]
    [SpacetimeDB.OnResult(typeof(PingRow), Table = "PingOutbox")]
    public static void OnPingResult(
        ReducerContext ctx,
        PingRow row,
        Result<Unit, string> result
    ) { }
}
