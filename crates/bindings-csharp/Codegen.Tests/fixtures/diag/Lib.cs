using System.ComponentModel;
using SpacetimeDB;

public enum LocalEnum { }

[SpacetimeDB.Type]
public partial struct TestUnsupportedType
{
    public DateTime UnsupportedSpecialType;
    public Exception UnsupportedSystemType;
    public UnresolvedType UnresolvedType;
    public LocalEnum UnsupportedEnum;
}

[SpacetimeDB.Type]
public enum TestEnumWithExplicitValues
{
    EnumVariant1 = 1,
    EnumVariant2 = 2,
}

[SpacetimeDB.Type]
public enum TestEnumWithTooManyVariants
{
    EnumVariant1,
    EnumVariant2,
    EnumVariant3,
    EnumVariant4,
    EnumVariant5,
    EnumVariant6,
    EnumVariant7,
    EnumVariant8,
    EnumVariant9,
    EnumVariant10,
    EnumVariant11,
    EnumVariant12,
    EnumVariant13,
    EnumVariant14,
    EnumVariant15,
    EnumVariant16,
    EnumVariant17,
    EnumVariant18,
    EnumVariant19,
    EnumVariant20,
    EnumVariant21,
    EnumVariant22,
    EnumVariant23,
    EnumVariant24,
    EnumVariant25,
    EnumVariant26,
    EnumVariant27,
    EnumVariant28,
    EnumVariant29,
    EnumVariant30,
    EnumVariant31,
    EnumVariant32,
    EnumVariant33,
    EnumVariant34,
    EnumVariant35,
    EnumVariant36,
    EnumVariant37,
    EnumVariant38,
    EnumVariant39,
    EnumVariant40,
    EnumVariant41,
    EnumVariant42,
    EnumVariant43,
    EnumVariant44,
    EnumVariant45,
    EnumVariant46,
    EnumVariant47,
    EnumVariant48,
    EnumVariant49,
    EnumVariant50,
    EnumVariant51,
    EnumVariant52,
    EnumVariant53,
    EnumVariant54,
    EnumVariant55,
    EnumVariant56,
    EnumVariant57,
    EnumVariant58,
    EnumVariant59,
    EnumVariant60,
    EnumVariant61,
    EnumVariant62,
    EnumVariant63,
    EnumVariant64,
    EnumVariant65,
    EnumVariant66,
    EnumVariant67,
    EnumVariant68,
    EnumVariant69,
    EnumVariant70,
    EnumVariant71,
    EnumVariant72,
    EnumVariant73,
    EnumVariant74,
    EnumVariant75,
    EnumVariant76,
    EnumVariant77,
    EnumVariant78,
    EnumVariant79,
    EnumVariant80,
    EnumVariant81,
    EnumVariant82,
    EnumVariant83,
    EnumVariant84,
    EnumVariant85,
    EnumVariant86,
    EnumVariant87,
    EnumVariant88,
    EnumVariant89,
    EnumVariant90,
    EnumVariant91,
    EnumVariant92,
    EnumVariant93,
    EnumVariant94,
    EnumVariant95,
    EnumVariant96,
    EnumVariant97,
    EnumVariant98,
    EnumVariant99,
    EnumVariant100,
    EnumVariant101,
    EnumVariant102,
    EnumVariant103,
    EnumVariant104,
    EnumVariant105,
    EnumVariant106,
    EnumVariant107,
    EnumVariant108,
    EnumVariant109,
    EnumVariant110,
    EnumVariant111,
    EnumVariant112,
    EnumVariant113,
    EnumVariant114,
    EnumVariant115,
    EnumVariant116,
    EnumVariant117,
    EnumVariant118,
    EnumVariant119,
    EnumVariant120,
    EnumVariant121,
    EnumVariant122,
    EnumVariant123,
    EnumVariant124,
    EnumVariant125,
    EnumVariant126,
    EnumVariant127,
    EnumVariant128,
    EnumVariant129,
    EnumVariant130,
    EnumVariant131,
    EnumVariant132,
    EnumVariant133,
    EnumVariant134,
    EnumVariant135,
    EnumVariant136,
    EnumVariant137,
    EnumVariant138,
    EnumVariant139,
    EnumVariant140,
    EnumVariant141,
    EnumVariant142,
    EnumVariant143,
    EnumVariant144,
    EnumVariant145,
    EnumVariant146,
    EnumVariant147,
    EnumVariant148,
    EnumVariant149,
    EnumVariant150,
    EnumVariant151,
    EnumVariant152,
    EnumVariant153,
    EnumVariant154,
    EnumVariant155,
    EnumVariant156,
    EnumVariant157,
    EnumVariant158,
    EnumVariant159,
    EnumVariant160,
    EnumVariant161,
    EnumVariant162,
    EnumVariant163,
    EnumVariant164,
    EnumVariant165,
    EnumVariant166,
    EnumVariant167,
    EnumVariant168,
    EnumVariant169,
    EnumVariant170,
    EnumVariant171,
    EnumVariant172,
    EnumVariant173,
    EnumVariant174,
    EnumVariant175,
    EnumVariant176,
    EnumVariant177,
    EnumVariant178,
    EnumVariant179,
    EnumVariant180,
    EnumVariant181,
    EnumVariant182,
    EnumVariant183,
    EnumVariant184,
    EnumVariant185,
    EnumVariant186,
    EnumVariant187,
    EnumVariant188,
    EnumVariant189,
    EnumVariant190,
    EnumVariant191,
    EnumVariant192,
    EnumVariant193,
    EnumVariant194,
    EnumVariant195,
    EnumVariant196,
    EnumVariant197,
    EnumVariant198,
    EnumVariant199,
    EnumVariant200,
    EnumVariant201,
    EnumVariant202,
    EnumVariant203,
    EnumVariant204,
    EnumVariant205,
    EnumVariant206,
    EnumVariant207,
    EnumVariant208,
    EnumVariant209,
    EnumVariant210,
    EnumVariant211,
    EnumVariant212,
    EnumVariant213,
    EnumVariant214,
    EnumVariant215,
    EnumVariant216,
    EnumVariant217,
    EnumVariant218,
    EnumVariant219,
    EnumVariant220,
    EnumVariant221,
    EnumVariant222,
    EnumVariant223,
    EnumVariant224,
    EnumVariant225,
    EnumVariant226,
    EnumVariant227,
    EnumVariant228,
    EnumVariant229,
    EnumVariant230,
    EnumVariant231,
    EnumVariant232,
    EnumVariant233,
    EnumVariant234,
    EnumVariant235,
    EnumVariant236,
    EnumVariant237,
    EnumVariant238,
    EnumVariant239,
    EnumVariant240,
    EnumVariant241,
    EnumVariant242,
    EnumVariant243,
    EnumVariant244,
    EnumVariant245,
    EnumVariant246,
    EnumVariant247,
    EnumVariant248,
    EnumVariant249,
    EnumVariant250,
    EnumVariant251,
    EnumVariant252,
    EnumVariant253,
    EnumVariant254,
    EnumVariant255,
    EnumVariant256,
    EnumVariant257,
}

[SpacetimeDB.Type]
public partial record TestTaggedEnumInlineTuple : SpacetimeDB.TaggedEnum<ValueTuple<int>>
{
    public int ForbiddenTaggedEnumField;
}

[SpacetimeDB.Type]
public partial record TestTaggedEnumField : SpacetimeDB.TaggedEnum<(int X, int Y)>
{
    public int ForbiddenField;
}

[SpacetimeDB.Type]
public partial struct TestTypeParams<T>
{
    public T Field;
}

public static partial class Reducers
{
    [SpacetimeDB.Reducer]
    public static int TestReducerReturnType(ReducerContext ctx) => 0;

    [SpacetimeDB.Reducer]
    public static void TestReducerWithoutContext() { }

    [SpacetimeDB.Reducer(ReducerKind.Init)]
    public static void TestDuplicateReducerKind1(ReducerContext ctx) { }

    [SpacetimeDB.Reducer(ReducerKind.Init)]
    public static void TestDuplicateReducerKind2(ReducerContext ctx) { }

    [SpacetimeDB.Reducer]
    public static void TestDuplicateReducerName(ReducerContext ctx) { }

    public static partial class InAnotherNamespace
    {
        [SpacetimeDB.Reducer]
        public static void TestDuplicateReducerName(ReducerContext ctx) { }
    }

    [SpacetimeDB.Reducer]
    public static void OnReducerWithReservedPrefix(ReducerContext ctx) { }

    [SpacetimeDB.Reducer]
    public static void __ReducerWithReservedPrefix(ReducerContext ctx) { }
}

[SpacetimeDB.Table]
public partial struct TestAutoIncNotInteger
{
    [AutoInc]
    public float AutoIncField;

    [Unique]
    [AutoInc]
    public string IdentityField;
}

[SpacetimeDB.Table]
public partial struct TestUniqueNotEquatable
{
    [Unique]
    public int? UniqueField;

    [PrimaryKey]
    public TestEnumWithExplicitValues PrimaryKeyField;
}

[SpacetimeDB.Table]
public partial record TestTableTaggedEnum : SpacetimeDB.TaggedEnum<(int X, int Y)> { }

[SpacetimeDB.Table]
public partial struct TestDuplicateTableName { }

public static partial class InAnotherNamespace
{
    [SpacetimeDB.Table]
    public partial struct TestDuplicateTableName { }
}

[SpacetimeDB.Table]
public partial struct TestDefaultFieldValues
{
    [Unique]
    public int? UniqueField;

    [Default("A default string set by attribute")]
    public string DefaultString = "";

    [Default(true)]
    public bool DefaultBool = false;

    [Default((sbyte)2)]
    public sbyte DefaultI8 = 1;

    [Default((byte)2)]
    public byte DefaultU8 = 1;

    [Default((short)2)]
    public short DefaultI16 = 1;

    [Default((ushort)2)]
    public ushort DefaultU16 = 1;

    [Default(2)]
    public int DefaultI32 = 1;

    [Default(2U)]
    public uint DefaultU32 = 1U;

    [Default(2L)]
    public long DefaultI64 = 1L;

    [Default(2UL)]
    public ulong DefaultU64 = 1UL;

    [Default(0x02)]
    public int DefaultHex = 1;

    [Default(0b00000010)]
    public int DefaultBin = 1;

    [Default(2.0f)]
    public float DefaultF32 = 1.0f;

    [Default(2.0)]
    public double DefaultF64 = 1.0;

    [Default(MyEnum.SetByAttribute)]
    public MyEnum DefaultEnum = MyEnum.SetByInitalization;

    [Default(null!)]
    public MyStruct? DefaultNull = new MyStruct(1);
}

[SpacetimeDB.Type]
public enum MyEnum
{
    Default,
    SetByInitalization,
    SetByAttribute,
}

[SpacetimeDB.Type]
public partial struct MyStruct
{
    public int x;

    public MyStruct(int x)
    {
        this.x = x;
    }
}

[SpacetimeDB.Table]
[SpacetimeDB.Index.BTree(Name = "TestIndexWithoutColumns")]
[SpacetimeDB.Index.BTree(Name = "TestIndexWithEmptyColumns", Columns = [])]
[SpacetimeDB.Index.BTree(Name = "TestUnknownColumns", Columns = ["UnknownColumn"])]
public partial struct TestIndexIssues
{
    [SpacetimeDB.Index.BTree(Name = "TestUnexpectedColumns", Columns = ["UnexpectedColumn"])]
    public int SelfIndexingColumn;
}

[SpacetimeDB.Table(
    Name = "TestScheduleWithoutPrimaryKey",
    Scheduled = "DummyScheduledReducer",
    ScheduledAt = nameof(ScheduleAtCorrectType)
)]
[SpacetimeDB.Table(
    Name = "TestScheduleWithWrongPrimaryKeyType",
    Scheduled = "DummyScheduledReducer",
    ScheduledAt = nameof(ScheduleAtCorrectType)
)]
[SpacetimeDB.Table(Name = "TestScheduleWithoutScheduleAt", Scheduled = "DummyScheduledReducer")]
[SpacetimeDB.Table(
    Name = "TestScheduleWithWrongScheduleAtType",
    Scheduled = "DummyScheduledReducer",
    ScheduledAt = nameof(ScheduleAtWrongType)
)]
[SpacetimeDB.Table(
    Name = "TestScheduleWithMissingScheduleAtField",
    Scheduled = "DummyScheduledReducer",
    ScheduledAt = "MissingField"
)]
public partial struct TestScheduleIssues
{
    [SpacetimeDB.PrimaryKey(Table = "TestScheduleWithWrongPrimaryKeyType")]
    public string IdWrongType;

    [SpacetimeDB.PrimaryKey(Table = "TestScheduleWithoutScheduleAt")]
    [SpacetimeDB.PrimaryKey(Table = "TestScheduleWithWrongScheduleAtType")]
    public int IdCorrectType;

    public int ScheduleAtWrongType;
    public ScheduleAt ScheduleAtCorrectType;

    [SpacetimeDB.Reducer]
    public static void DummyScheduledReducer(ReducerContext ctx, TestScheduleIssues table) { }
}

public partial class Module
{
#pragma warning disable STDB_UNSTABLE // Enable ClientVisibilityFilter

    // Invalid: not public static readonly
    [SpacetimeDB.ClientVisibilityFilter]
    private Filter MY_FILTER = new Filter.Sql("SELECT * FROM TestAutoIncNotInteger");

    // Invalid: not public static readonly
    [SpacetimeDB.ClientVisibilityFilter]
    public static Filter MY_SECOND_FILTER = new Filter.Sql("SELECT * FROM TestAutoIncNotInteger");

    // Invalid: not a Filter
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly string MY_THIRD_FILTER = "SELECT * FROM TestAutoIncNotInteger";

#pragma warning restore STDB_UNSTABLE // Disable ClientVisibilityFilter

    // Valid Filter, but [ClientVisibilityFilter] is disabled
    [SpacetimeDB.ClientVisibilityFilter]
    public static readonly Filter MY_FOURTH_FILTER = new Filter.Sql(
        "SELECT * FROM TestAutoIncNotInteger"
    );
}
