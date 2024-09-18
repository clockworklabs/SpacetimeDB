namespace SpacetimeDB.Sdk.Test;

using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Type]
    public enum SimpleEnum
    {
        Zero,
        One,
        Two,
    }

    [SpacetimeDB.Type]
    public partial record EnumWithPayload
        : SpacetimeDB.TaggedEnum<(
            byte U8,
            ushort U16,
            uint U32,
            ulong U64,
            U128 U128,
            U256 U256,
            sbyte I8,
            short I16,
            int I32,
            long I64,
            I128 I128,
            I256 I256,
            bool Bool,
            float F32,
            double F64,
            string Str,
            Identity Identity,
            Address Address,
            List<byte> Bytes,
            List<int> Ints,
            List<string> Strings,
            List<SimpleEnum> SimpleEnums
        )>;

    [SpacetimeDB.Type]
    public partial struct UnitStruct { }

    [SpacetimeDB.Type]
    public partial struct ByteStruct
    {
        public byte b;
    }

    [SpacetimeDB.Type]
    public partial struct EveryPrimitiveStruct
    {
        public byte a;
        public ushort b;
        public uint c;
        public ulong d;
        public U128 e;
        public U256 f;
        public sbyte g;
        public short h;
        public int i;
        public long j;
        public I128 k;
        public I256 l;
        public bool m;
        public float n;
        public double o;
        public string p;
        public Identity q;
        public Address r;
    }

    [SpacetimeDB.Type]
    public partial struct EveryVecStruct
    {
        public List<byte> a;
        public List<ushort> b;
        public List<uint> c;
        public List<ulong> d;
        public List<U128> e;
        public List<U256> f;
        public List<sbyte> g;
        public List<short> h;
        public List<int> i;
        public List<long> j;
        public List<I128> k;
        public List<I256> l;
        public List<bool> m;
        public List<float> n;
        public List<double> o;
        public List<string> p;
        public List<Identity> q;
        public List<Address> r;
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneU8
    {
        public byte n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_u8(ReducerContext ctx, byte n)
    {
        var row = new OneU8 { n = n };
        ctx.Db.OneU8.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneU16
    {
        public ushort n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_u16(ReducerContext ctx, ushort n)
    {
        var row = new OneU16 { n = n };
        ctx.Db.OneU16.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneU32
    {
        public uint n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_u32(ReducerContext ctx, uint n)
    {
        var row = new OneU32 { n = n };
        ctx.Db.OneU32.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneU64
    {
        public ulong n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_u64(ReducerContext ctx, ulong n)
    {
        var row = new OneU64 { n = n };
        ctx.Db.OneU64.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneU128
    {
        public U128 n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_u128(ReducerContext ctx, U128 n)
    {
        var row = new OneU128 { n = n };
        ctx.Db.OneU128.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneU256
    {
        public U256 n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_u256(ReducerContext ctx, U256 n)
    {
        var row = new OneU256 { n = n };
        ctx.Db.OneU256.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneI8
    {
        public sbyte n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_i8(ReducerContext ctx, sbyte n)
    {
        var row = new OneI8 { n = n };
        ctx.Db.OneI8.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneI16
    {
        public short n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_i16(ReducerContext ctx, short n)
    {
        var row = new OneI16 { n = n };
        ctx.Db.OneI16.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneI32
    {
        public int n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_i32(ReducerContext ctx, int n)
    {
        var row = new OneI32 { n = n };
        ctx.Db.OneI32.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneI64
    {
        public long n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_i64(ReducerContext ctx, long n)
    {
        var row = new OneI64 { n = n };
        ctx.Db.OneI64.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneI128
    {
        public I128 n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_i128(ReducerContext ctx, I128 n)
    {
        var row = new OneI128 { n = n };
        ctx.Db.OneI128.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneI256
    {
        public I256 n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_i256(ReducerContext ctx, I256 n)
    {
        var row = new OneI256 { n = n };
        ctx.Db.OneI256.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneBool
    {
        public bool b;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_bool(ReducerContext ctx, bool b)
    {
        var row = new OneBool { b = b };
        ctx.Db.OneBool.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneF32
    {
        public float f;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_f32(ReducerContext ctx, float f)
    {
        var row = new OneF32 { f = f };
        ctx.Db.OneF32.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneF64
    {
        public double f;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_f64(ReducerContext ctx, double f)
    {
        var row = new OneF64 { f = f };
        ctx.Db.OneF64.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneString
    {
        public string s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_string(ReducerContext ctx, string s)
    {
        var row = new OneString { s = s };
        ctx.Db.OneString.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneIdentity
    {
        public Identity i;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_identity(ReducerContext ctx, Identity i)
    {
        var row = new OneIdentity { i = i };
        ctx.Db.OneIdentity.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneAddress
    {
        public Address a;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_address(ReducerContext ctx, Address a)
    {
        var row = new OneAddress { a = a };
        ctx.Db.OneAddress.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneSimpleEnum
    {
        public SimpleEnum e;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_simple_enum(ReducerContext ctx, SimpleEnum e)
    {
        var row = new OneSimpleEnum { e = e };
        ctx.Db.OneSimpleEnum.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneEnumWithPayload
    {
        public EnumWithPayload e;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_enum_with_payload(ReducerContext ctx, EnumWithPayload e)
    {
        var row = new OneEnumWithPayload { e = e };
        ctx.Db.OneEnumWithPayload.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneUnitStruct
    {
        public UnitStruct s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_unit_struct(ReducerContext ctx, UnitStruct s)
    {
        var row = new OneUnitStruct { s = s };
        ctx.Db.OneUnitStruct.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneByteStruct
    {
        public ByteStruct s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_byte_struct(ReducerContext ctx, ByteStruct s)
    {
        var row = new OneByteStruct { s = s };
        ctx.Db.OneByteStruct.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneEveryPrimitiveStruct
    {
        public EveryPrimitiveStruct s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_every_primitive_struct(ReducerContext ctx, EveryPrimitiveStruct s)
    {
        var row = new OneEveryPrimitiveStruct { s = s };
        ctx.Db.OneEveryPrimitiveStruct.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OneEveryVecStruct
    {
        public EveryVecStruct s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_every_vec_struct(ReducerContext ctx, EveryVecStruct s)
    {
        var row = new OneEveryVecStruct { s = s };
        ctx.Db.OneEveryVecStruct.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecU8
    {
        public List<byte> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_u8(ReducerContext ctx, List<byte> n)
    {
        var row = new VecU8 { n = n };
        ctx.Db.VecU8.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecU16
    {
        public List<ushort> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_u16(ReducerContext ctx, List<ushort> n)
    {
        var row = new VecU16 { n = n };
        ctx.Db.VecU16.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecU32
    {
        public List<uint> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_u32(ReducerContext ctx, List<uint> n)
    {
        var row = new VecU32 { n = n };
        ctx.Db.VecU32.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecU64
    {
        public List<ulong> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_u64(ReducerContext ctx, List<ulong> n)
    {
        var row = new VecU64 { n = n };
        ctx.Db.VecU64.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecU128
    {
        public List<U128> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_u128(ReducerContext ctx, List<U128> n)
    {
        var row = new VecU128 { n = n };
        ctx.Db.VecU128.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecU256
    {
        public List<U256> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_u256(ReducerContext ctx, List<U256> n)
    {
        var row = new VecU256 { n = n };
        ctx.Db.VecU256.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecI8
    {
        public List<sbyte> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_i8(ReducerContext ctx, List<sbyte> n)
    {
        var row = new VecI8 { n = n };
        ctx.Db.VecI8.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecI16
    {
        public List<short> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_i16(ReducerContext ctx, List<short> n)
    {
        var row = new VecI16 { n = n };
        ctx.Db.VecI16.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecI32
    {
        public List<int> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_i32(ReducerContext ctx, List<int> n)
    {
        var row = new VecI32 { n = n };
        ctx.Db.VecI32.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecI64
    {
        public List<long> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_i64(ReducerContext ctx, List<long> n)
    {
        var row = new VecI64 { n = n };
        ctx.Db.VecI64.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecI128
    {
        public List<I128> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_i128(ReducerContext ctx, List<I128> n)
    {
        var row = new VecI128 { n = n };
        ctx.Db.VecI128.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecI256
    {
        public List<I256> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_i256(ReducerContext ctx, List<I256> n)
    {
        var row = new VecI256 { n = n };
        ctx.Db.VecI256.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecBool
    {
        public List<bool> b;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_bool(ReducerContext ctx, List<bool> b)
    {
        var row = new VecBool { b = b };
        ctx.Db.VecBool.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecF32
    {
        public List<float> f;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_f32(ReducerContext ctx, List<float> f)
    {
        var row = new VecF32 { f = f };
        ctx.Db.VecF32.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecF64
    {
        public List<double> f;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_f64(ReducerContext ctx, List<double> f)
    {
        var row = new VecF64 { f = f };
        ctx.Db.VecF64.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecString
    {
        public List<string> s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_string(ReducerContext ctx, List<string> s)
    {
        var row = new VecString { s = s };
        ctx.Db.VecString.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecIdentity
    {
        public List<Identity> i;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_identity(ReducerContext ctx, List<Identity> i)
    {
        var row = new VecIdentity { i = i };
        ctx.Db.VecIdentity.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecAddress
    {
        public List<Address> a;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_address(ReducerContext ctx, List<Address> a)
    {
        var row = new VecAddress { a = a };
        ctx.Db.VecAddress.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecSimpleEnum
    {
        public List<SimpleEnum> e;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_simple_enum(ReducerContext ctx, List<SimpleEnum> e)
    {
        var row = new VecSimpleEnum { e = e };
        ctx.Db.VecSimpleEnum.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecEnumWithPayload
    {
        public List<EnumWithPayload> e;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_enum_with_payload(ReducerContext ctx, List<EnumWithPayload> e)
    {
        var row = new VecEnumWithPayload { e = e };
        ctx.Db.VecEnumWithPayload.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecUnitStruct
    {
        public List<UnitStruct> s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_unit_struct(ReducerContext ctx, List<UnitStruct> s)
    {
        var row = new VecUnitStruct { s = s };
        ctx.Db.VecUnitStruct.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecByteStruct
    {
        public List<ByteStruct> s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_byte_struct(ReducerContext ctx, List<ByteStruct> s)
    {
        var row = new VecByteStruct { s = s };
        ctx.Db.VecByteStruct.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecEveryPrimitiveStruct
    {
        public List<EveryPrimitiveStruct> s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_every_primitive_struct(ReducerContext ctx, List<EveryPrimitiveStruct> s)
    {
        var row = new VecEveryPrimitiveStruct { s = s };
        ctx.Db.VecEveryPrimitiveStruct.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct VecEveryVecStruct
    {
        public List<EveryVecStruct> s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_every_vec_struct(ReducerContext ctx, List<EveryVecStruct> s)
    {
        var row = new VecEveryVecStruct { s = s };
        ctx.Db.VecEveryVecStruct.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OptionI32
    {
        public int? n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_option_i32(ReducerContext ctx, int? n)
    {
        var row = new OptionI32 { n = n };
        ctx.Db.OptionI32.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OptionString
    {
        public string? s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_option_string(ReducerContext ctx, string? s)
    {
        var row = new OptionString { s = s };
        ctx.Db.OptionString.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OptionIdentity
    {
        public Identity? i;
    }

    [SpacetimeDB.Reducer]
    public static void insert_option_identity(ReducerContext ctx, Identity? i)
    {
        var row = new OptionIdentity { i = i };
        ctx.Db.OptionIdentity.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OptionSimpleEnum
    {
        public SimpleEnum? e;
    }

    [SpacetimeDB.Reducer]
    public static void insert_option_simple_enum(ReducerContext ctx, SimpleEnum? e)
    {
        var row = new OptionSimpleEnum { e = e };
        ctx.Db.OptionSimpleEnum.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OptionEveryPrimitiveStruct
    {
        public EveryPrimitiveStruct? s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_option_every_primitive_struct(ReducerContext ctx, EveryPrimitiveStruct? s)
    {
        var row = new OptionEveryPrimitiveStruct { s = s };
        ctx.Db.OptionEveryPrimitiveStruct.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct OptionVecOptionI32
    {
        public List<int?>? v;
    }

    [SpacetimeDB.Reducer]
    public static void insert_option_vec_option_i32(ReducerContext ctx, List<int?>? v)
    {
        var row = new OptionVecOptionI32 { v = v };
        ctx.Db.OptionVecOptionI32.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueU8
    {
        [SpacetimeDB.Unique]
        public byte n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_u8(ReducerContext ctx, byte n, int data)
    {
        var row = new UniqueU8 { n = n, data = data };
        ctx.Db.UniqueU8.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_u8(ReducerContext ctx, byte n, int data)
    {
        var key = n;
        var row = new UniqueU8 { n = n, data = data };
        ctx.Db.UniqueU8.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_u8(ReducerContext ctx, byte n)
    {
        ctx.Db.UniqueU8.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueU16
    {
        [SpacetimeDB.Unique]
        public ushort n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_u16(ReducerContext ctx, ushort n, int data)
    {
        var row = new UniqueU16 { n = n, data = data };
        ctx.Db.UniqueU16.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_u16(ReducerContext ctx, ushort n, int data)
    {
        var key = n;
        var row = new UniqueU16 { n = n, data = data };
        ctx.Db.UniqueU16.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_u16(ReducerContext ctx, ushort n)
    {
        ctx.Db.UniqueU16.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueU32
    {
        [SpacetimeDB.Unique]
        public uint n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_u32(ReducerContext ctx, uint n, int data)
    {
        var row = new UniqueU32 { n = n, data = data };
        ctx.Db.UniqueU32.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_u32(ReducerContext ctx, uint n, int data)
    {
        var key = n;
        var row = new UniqueU32 { n = n, data = data };
        ctx.Db.UniqueU32.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_u32(ReducerContext ctx, uint n)
    {
        ctx.Db.UniqueU32.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueU64
    {
        [SpacetimeDB.Unique]
        public ulong n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_u64(ReducerContext ctx, ulong n, int data)
    {
        var row = new UniqueU64 { n = n, data = data };
        ctx.Db.UniqueU64.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_u64(ReducerContext ctx, ulong n, int data)
    {
        var key = n;
        var row = new UniqueU64 { n = n, data = data };
        ctx.Db.UniqueU64.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_u64(ReducerContext ctx, ulong n)
    {
        ctx.Db.UniqueU64.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueU128
    {
        [SpacetimeDB.Unique]
        public U128 n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_u128(ReducerContext ctx, U128 n, int data)
    {
        var row = new UniqueU128 { n = n, data = data };
        ctx.Db.UniqueU128.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_u128(ReducerContext ctx, U128 n, int data)
    {
        var key = n;
        var row = new UniqueU128 { n = n, data = data };
        ctx.Db.UniqueU128.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_u128(ReducerContext ctx, U128 n)
    {
        ctx.Db.UniqueU128.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueU256
    {
        [SpacetimeDB.Unique]
        public U256 n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_u256(ReducerContext ctx, U256 n, int data)
    {
        var row = new UniqueU256 { n = n, data = data };
        ctx.Db.UniqueU256.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_u256(ReducerContext ctx, U256 n, int data)
    {
        var key = n;
        var row = new UniqueU256 { n = n, data = data };
        ctx.Db.UniqueU256.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_u256(ReducerContext ctx, U256 n)
    {
        ctx.Db.UniqueU256.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueI8
    {
        [SpacetimeDB.Unique]
        public sbyte n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_i8(ReducerContext ctx, sbyte n, int data)
    {
        var row = new UniqueI8 { n = n, data = data };
        ctx.Db.UniqueI8.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_i8(ReducerContext ctx, sbyte n, int data)
    {
        var key = n;
        var row = new UniqueI8 { n = n, data = data };
        ctx.Db.UniqueI8.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_i8(ReducerContext ctx, sbyte n)
    {
        ctx.Db.UniqueI8.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueI16
    {
        [SpacetimeDB.Unique]
        public short n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_i16(ReducerContext ctx, short n, int data)
    {
        var row = new UniqueI16 { n = n, data = data };
        ctx.Db.UniqueI16.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_i16(ReducerContext ctx, short n, int data)
    {
        var key = n;
        var row = new UniqueI16 { n = n, data = data };
        ctx.Db.UniqueI16.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_i16(ReducerContext ctx, short n)
    {
        ctx.Db.UniqueI16.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueI32
    {
        [SpacetimeDB.Unique]
        public int n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_i32(ReducerContext ctx, int n, int data)
    {
        var row = new UniqueI32 { n = n, data = data };
        ctx.Db.UniqueI32.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_i32(ReducerContext ctx, int n, int data)
    {
        var key = n;
        var row = new UniqueI32 { n = n, data = data };
        ctx.Db.UniqueI32.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_i32(ReducerContext ctx, int n)
    {
        ctx.Db.UniqueI32.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueI64
    {
        [SpacetimeDB.Unique]
        public long n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_i64(ReducerContext ctx, long n, int data)
    {
        var row = new UniqueI64 { n = n, data = data };
        ctx.Db.UniqueI64.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_i64(ReducerContext ctx, long n, int data)
    {
        var key = n;
        var row = new UniqueI64 { n = n, data = data };
        ctx.Db.UniqueI64.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_i64(ReducerContext ctx, long n)
    {
        ctx.Db.UniqueI64.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueI128
    {
        [SpacetimeDB.Unique]
        public I128 n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_i128(ReducerContext ctx, I128 n, int data)
    {
        var row = new UniqueI128 { n = n, data = data };
        ctx.Db.UniqueI128.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_i128(ReducerContext ctx, I128 n, int data)
    {
        var key = n;
        var row = new UniqueI128 { n = n, data = data };
        ctx.Db.UniqueI128.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_i128(ReducerContext ctx, I128 n)
    {
        ctx.Db.UniqueI128.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueI256
    {
        [SpacetimeDB.Unique]
        public I256 n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_i256(ReducerContext ctx, I256 n, int data)
    {
        var row = new UniqueI256 { n = n, data = data };
        ctx.Db.UniqueI256.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_i256(ReducerContext ctx, I256 n, int data)
    {
        var key = n;
        var row = new UniqueI256 { n = n, data = data };
        ctx.Db.UniqueI256.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_i256(ReducerContext ctx, I256 n)
    {
        ctx.Db.UniqueI256.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueBool
    {
        [SpacetimeDB.Unique]
        public bool b;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_bool(ReducerContext ctx, bool b, int data)
    {
        var row = new UniqueBool { b = b, data = data };
        ctx.Db.UniqueBool.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_bool(ReducerContext ctx, bool b, int data)
    {
        var key = b;
        var row = new UniqueBool { b = b, data = data };
        ctx.Db.UniqueBool.UpdateByb(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_bool(ReducerContext ctx, bool b)
    {
        ctx.Db.UniqueBool.DeleteByb(b);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueString
    {
        [SpacetimeDB.Unique]
        public string s;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_string(ReducerContext ctx, string s, int data)
    {
        var row = new UniqueString { s = s, data = data };
        ctx.Db.UniqueString.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_string(ReducerContext ctx, string s, int data)
    {
        var key = s;
        var row = new UniqueString { s = s, data = data };
        ctx.Db.UniqueString.UpdateBys(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_string(ReducerContext ctx, string s)
    {
        ctx.Db.UniqueString.DeleteBys(s);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueIdentity
    {
        [SpacetimeDB.Unique]
        public Identity i;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_identity(ReducerContext ctx, Identity i, int data)
    {
        var row = new UniqueIdentity { i = i, data = data };
        ctx.Db.UniqueIdentity.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_identity(ReducerContext ctx, Identity i, int data)
    {
        var key = i;
        var row = new UniqueIdentity { i = i, data = data };
        ctx.Db.UniqueIdentity.UpdateByi(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_identity(ReducerContext ctx, Identity i)
    {
        ctx.Db.UniqueIdentity.DeleteByi(i);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct UniqueAddress
    {
        [SpacetimeDB.Unique]
        public Address a;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_address(ReducerContext ctx, Address a, int data)
    {
        var row = new UniqueAddress { a = a, data = data };
        ctx.Db.UniqueAddress.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_address(ReducerContext ctx, Address a, int data)
    {
        var key = a;
        var row = new UniqueAddress { a = a, data = data };
        ctx.Db.UniqueAddress.UpdateBya(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_address(ReducerContext ctx, Address a)
    {
        ctx.Db.UniqueAddress.DeleteBya(a);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkU8
    {
        [SpacetimeDB.PrimaryKey]
        public byte n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_u8(ReducerContext ctx, byte n, int data)
    {
        var row = new PkU8 { n = n, data = data };
        ctx.Db.PkU8.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_u8(ReducerContext ctx, byte n, int data)
    {
        var key = n;
        var row = new PkU8 { n = n, data = data };
        ctx.Db.PkU8.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_u8(ReducerContext ctx, byte n)
    {
        ctx.Db.PkU8.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkU16
    {
        [SpacetimeDB.PrimaryKey]
        public ushort n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_u16(ReducerContext ctx, ushort n, int data)
    {
        var row = new PkU16 { n = n, data = data };
        ctx.Db.PkU16.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_u16(ReducerContext ctx, ushort n, int data)
    {
        var key = n;
        var row = new PkU16 { n = n, data = data };
        ctx.Db.PkU16.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_u16(ReducerContext ctx, ushort n)
    {
        ctx.Db.PkU16.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkU32
    {
        [SpacetimeDB.PrimaryKey]
        public uint n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_u32(ReducerContext ctx, uint n, int data)
    {
        var row = new PkU32 { n = n, data = data };
        ctx.Db.PkU32.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_u32(ReducerContext ctx, uint n, int data)
    {
        var key = n;
        var row = new PkU32 { n = n, data = data };
        ctx.Db.PkU32.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_u32(ReducerContext ctx, uint n)
    {
        ctx.Db.PkU32.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkU64
    {
        [SpacetimeDB.PrimaryKey]
        public ulong n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_u64(ReducerContext ctx, ulong n, int data)
    {
        var row = new PkU64 { n = n, data = data };
        ctx.Db.PkU64.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_u64(ReducerContext ctx, ulong n, int data)
    {
        var key = n;
        var row = new PkU64 { n = n, data = data };
        ctx.Db.PkU64.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_u64(ReducerContext ctx, ulong n)
    {
        ctx.Db.PkU64.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkU128
    {
        [SpacetimeDB.PrimaryKey]
        public U128 n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_u128(ReducerContext ctx, U128 n, int data)
    {
        var row = new PkU128 { n = n, data = data };
        ctx.Db.PkU128.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_u128(ReducerContext ctx, U128 n, int data)
    {
        var key = n;
        var row = new PkU128 { n = n, data = data };
        ctx.Db.PkU128.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_u128(ReducerContext ctx, U128 n)
    {
        ctx.Db.PkU128.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkU256
    {
        [SpacetimeDB.PrimaryKey]
        public U256 n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_u256(ReducerContext ctx, U256 n, int data)
    {
        var row = new PkU256 { n = n, data = data };
        ctx.Db.PkU256.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_u256(ReducerContext ctx, U256 n, int data)
    {
        var key = n;
        var row = new PkU256 { n = n, data = data };
        ctx.Db.PkU256.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_u256(ReducerContext ctx, U256 n)
    {
        ctx.Db.PkU256.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkI8
    {
        [SpacetimeDB.PrimaryKey]
        public sbyte n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_i8(ReducerContext ctx, sbyte n, int data)
    {
        var row = new PkI8 { n = n, data = data };
        ctx.Db.PkI8.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_i8(ReducerContext ctx, sbyte n, int data)
    {
        var key = n;
        var row = new PkI8 { n = n, data = data };
        ctx.Db.PkI8.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_i8(ReducerContext ctx, sbyte n)
    {
        ctx.Db.PkI8.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkI16
    {
        [SpacetimeDB.PrimaryKey]
        public short n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_i16(ReducerContext ctx, short n, int data)
    {
        var row = new PkI16 { n = n, data = data };
        ctx.Db.PkI16.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_i16(ReducerContext ctx, short n, int data)
    {
        var key = n;
        var row = new PkI16 { n = n, data = data };
        ctx.Db.PkI16.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_i16(ReducerContext ctx, short n)
    {
        ctx.Db.PkI16.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkI32
    {
        [SpacetimeDB.PrimaryKey]
        public int n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_i32(ReducerContext ctx, int n, int data)
    {
        var row = new PkI32 { n = n, data = data };
        ctx.Db.PkI32.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_i32(ReducerContext ctx, int n, int data)
    {
        var key = n;
        var row = new PkI32 { n = n, data = data };
        ctx.Db.PkI32.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_i32(ReducerContext ctx, int n)
    {
        ctx.Db.PkI32.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkI64
    {
        [SpacetimeDB.PrimaryKey]
        public long n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_i64(ReducerContext ctx, long n, int data)
    {
        var row = new PkI64 { n = n, data = data };
        ctx.Db.PkI64.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_i64(ReducerContext ctx, long n, int data)
    {
        var key = n;
        var row = new PkI64 { n = n, data = data };
        ctx.Db.PkI64.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_i64(ReducerContext ctx, long n)
    {
        ctx.Db.PkI64.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkI128
    {
        [SpacetimeDB.PrimaryKey]
        public I128 n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_i128(ReducerContext ctx, I128 n, int data)
    {
        var row = new PkI128 { n = n, data = data };
        ctx.Db.PkI128.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_i128(ReducerContext ctx, I128 n, int data)
    {
        var key = n;
        var row = new PkI128 { n = n, data = data };
        ctx.Db.PkI128.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_i128(ReducerContext ctx, I128 n)
    {
        ctx.Db.PkI128.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkI256
    {
        [SpacetimeDB.PrimaryKey]
        public I256 n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_i256(ReducerContext ctx, I256 n, int data)
    {
        var row = new PkI256 { n = n, data = data };
        ctx.Db.PkI256.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_i256(ReducerContext ctx, I256 n, int data)
    {
        var key = n;
        var row = new PkI256 { n = n, data = data };
        ctx.Db.PkI256.UpdateByn(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_i256(ReducerContext ctx, I256 n)
    {
        ctx.Db.PkI256.DeleteByn(n);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkBool
    {
        [SpacetimeDB.PrimaryKey]
        public bool b;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_bool(ReducerContext ctx, bool b, int data)
    {
        var row = new PkBool { b = b, data = data };
        ctx.Db.PkBool.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_bool(ReducerContext ctx, bool b, int data)
    {
        var key = b;
        var row = new PkBool { b = b, data = data };
        ctx.Db.PkBool.UpdateByb(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_bool(ReducerContext ctx, bool b)
    {
        ctx.Db.PkBool.DeleteByb(b);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkString
    {
        [SpacetimeDB.PrimaryKey]
        public string s;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_string(ReducerContext ctx, string s, int data)
    {
        var row = new PkString { s = s, data = data };
        ctx.Db.PkString.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_string(ReducerContext ctx, string s, int data)
    {
        var key = s;
        var row = new PkString { s = s, data = data };
        ctx.Db.PkString.UpdateBys(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_string(ReducerContext ctx, string s)
    {
        ctx.Db.PkString.DeleteBys(s);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkIdentity
    {
        [SpacetimeDB.PrimaryKey]
        public Identity i;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_identity(ReducerContext ctx, Identity i, int data)
    {
        var row = new PkIdentity { i = i, data = data };
        ctx.Db.PkIdentity.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_identity(ReducerContext ctx, Identity i, int data)
    {
        var key = i;
        var row = new PkIdentity { i = i, data = data };
        ctx.Db.PkIdentity.UpdateByi(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_identity(ReducerContext ctx, Identity i)
    {
        ctx.Db.PkIdentity.DeleteByi(i);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct PkAddress
    {
        [SpacetimeDB.PrimaryKey]
        public Address a;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_address(ReducerContext ctx, Address a, int data)
    {
        var row = new PkAddress { a = a, data = data };
        ctx.Db.PkAddress.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_address(ReducerContext ctx, Address a, int data)
    {
        var key = a;
        var row = new PkAddress { a = a, data = data };
        ctx.Db.PkAddress.UpdateBya(key, ref row);
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_address(ReducerContext ctx, Address a)
    {
        ctx.Db.PkAddress.DeleteBya(a);
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_one_identity(ReducerContext ctx)
    {
        var row = new OneIdentity { i = ctx.Sender };
        ctx.Db.OneIdentity.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_vec_identity(ReducerContext ctx)
    {
        var row = new VecIdentity { i = new List<Identity> { ctx.Sender } };
        ctx.Db.VecIdentity.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_unique_identity(ReducerContext ctx, int data)
    {
        var row = new UniqueIdentity { i = ctx.Sender, data = data };
        ctx.Db.UniqueIdentity.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_pk_identity(ReducerContext ctx, int data)
    {
        var row = new PkIdentity { i = ctx.Sender, data = data };
        ctx.Db.PkIdentity.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_one_address(ReducerContext ctx)
    {
        var row = new OneAddress { a = (Address)ctx.Address!, };
        ctx.Db.OneAddress.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_vec_address(ReducerContext ctx)
    {
        // VecAddress::insert(VecAddress {
        //     < a[_]>::into_vec(
        //         #[rustc_box]
        //         ::alloc::boxed::Box::new([ctx.Address.context("No address in reducer context")?]),
        //     ),
        // });
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_unique_address(ReducerContext ctx, int data)
    {
        var row = new UniqueAddress { a = (Address)ctx.Address!, data = data };
        ctx.Db.UniqueAddress.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_pk_address(ReducerContext ctx, int data)
    {
        var row = new PkAddress { a = (Address)ctx.Address!, data = data };
        ctx.Db.PkAddress.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct LargeTable
    {
        public byte a;
        public ushort b;
        public uint c;
        public ulong d;
        public U128 e;
        public U256 f;
        public sbyte g;
        public short h;
        public int i;
        public long j;
        public I128 k;
        public I256 l;
        public bool m;
        public float n;
        public double o;
        public string p;
        public SimpleEnum q;
        public EnumWithPayload r;
        public UnitStruct s;
        public ByteStruct t;
        public EveryPrimitiveStruct u;
        public EveryVecStruct v;
    }

    [SpacetimeDB.Reducer]
    public static void insert_large_table(
        ReducerContext ctx,
        byte a,
        ushort b,
        uint c,
        ulong d,
        U128 e,
        U256 f,
        sbyte g,
        short h,
        int i,
        long j,
        I128 k,
        I256 l,
        bool m,
        float n,
        double o,
        string p,
        SimpleEnum q,
        EnumWithPayload r,
        UnitStruct s,
        ByteStruct t,
        EveryPrimitiveStruct u,
        EveryVecStruct v
    )
    {
        var row = new LargeTable {
            a = a,
            b = b,
            c = c,
            d = d,
            e = e,
            f = f,
            g = g,
            h = h,
            i = i,
            j = j,
            k = k,
            l = l,
            m = m,
            n = n,
            o = o,
            p = p,
            q = q,
            r = r,
            s = s,
            t = t,
            u = u,
            v = v,
        };
        ctx.Db.LargeTable.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void insert_primitives_as_strings(ReducerContext ctx, EveryPrimitiveStruct s)
    {
        var row = new VecString {
            s = typeof(EveryPrimitiveStruct)
                .GetFields()
                .Select(f => f.GetValue(s)!.ToString()!.ToLowerInvariant())
                .ToList()
        };
        ctx.Db.VecString.Insert(ref row);
    }

    [SpacetimeDB.Table(Public = true)]
    public partial struct TableHoldsTable
    {
        public OneU8 a;
        public VecU8 b;
    }

    [SpacetimeDB.Reducer]
    public static void insert_table_holds_table(ReducerContext ctx, OneU8 a, VecU8 b)
    {
        var row = new TableHoldsTable { a = a, b = b };
        ctx.Db.TableHoldsTable.Insert(ref row);
    }

    [SpacetimeDB.Reducer]
    public static void no_op_succeeds(ReducerContext ctx) { }
}
