
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "defaults_test_table", Public = true)]
    public partial struct DefaultsTestTable
    {
        public uint id;
        [Default(true)] public bool bool_value;
        [Default((sbyte)-8)] public sbyte i8_value;
        [Default((byte)8)] public byte u8_value;
        [Default((short)-16)] public short i16_value;
        [Default((ushort)16)] public ushort u16_value;
        [Default(-32)] public int i32_value;
        [Default(32U)] public uint u32_value;
        [Default(-64L)] public long i64_value;
        [Default(64UL)] public ulong u64_value;
        [Default(32.5f)] public float f32_positive_value;
        [Default(-32.5f)] public float f32_negative_value;
        [Default(64.25)] public double f64_positive_value;
        [Default(-64.25)] public double f64_negative_value;
        [Default("default string")] public string string_value;
    }
}
