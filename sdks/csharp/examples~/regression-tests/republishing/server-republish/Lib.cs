using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Name = "ExampleData", Public = true)]
    public partial struct ExampleData
    {
        [SpacetimeDB.PrimaryKey]
        public uint Primary;
        public uint TestPass;
        [Default("This is a default string")] public string DefaultString = "";
        [Default(true)] public bool DefaultBool = false;
        [Default((sbyte)2)] public sbyte DefaultI8 = 1;
        [Default((byte)2)] public byte DefaultU8 = 1;
        [Default((short)2)] public short DefaultI16 = 1;
        [Default((ushort)2)] public ushort DefaultU16 = 1;
        [Default(2)] public int DefaultI32 = 1;
        [Default(2U)] public uint DefaultU32 = 1U;
        [Default(2L)] public long DefaultI64 = 1L;
        [Default(2UL)] public ulong DefaultU64 = 1UL;
        [Default(0x02)] public int DefaultHex = 1;
        [Default(0b00000010)]  public int DefaultBin = 1;
        [Default(2.0f)] public float DefaultF32 = 1.0f;
        [Default(2.0)] public double DefaultF64 = 1.0;
        [Default(MyEnum.SetByAttribute)] public MyEnum DefaultEnum = MyEnum.SetByInitalization;
        [Default(null!)] public MyStruct? DefaultNull = new MyStruct(1);

        public ExampleData()
        {
        }
    }
    
    [SpacetimeDB.Type]
    public enum MyEnum
    {
        Default,
        SetByInitalization,
        SetByAttribute
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

    [SpacetimeDB.Reducer]
    public static void Insert(ReducerContext ctx, uint id)
    {
        var exampleData = ctx.Db.ExampleData.Insert(new ExampleData { Primary = id, TestPass = 2 });
        Log.Info($"Inserted key {exampleData.Primary} on pass {exampleData.TestPass}");
    }
}
