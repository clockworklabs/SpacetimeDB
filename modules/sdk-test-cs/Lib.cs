using SpacetimeDB.Module;
using static SpacetimeDB.Runtime;

static partial class Module
{
    [SpacetimeDB.Type]
    public enum SimpleEnum
    {
        Zero,
        One,
        Two,
    }

    [SpacetimeDB.Type]
    public partial struct EnumWithPayload
        : SpacetimeDB.TaggedEnum<(
            byte U8,
            ushort U16,
            uint U32,
            ulong U64,
            UInt128 U128,
            sbyte I8,
            short I16,
            int I32,
            long I64,
            Int128 I128,
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
        )> { }

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
        public UInt128 e;
        public sbyte f;
        public short g;
        public int h;
        public long i;
        public Int128 j;
        public bool k;
        public float l;
        public double m;
        public string n;
        public Identity o;
        public Address p;
    }

    [SpacetimeDB.Type]
    public partial struct EveryVecStruct
    {
        public List<byte> a;
        public List<ushort> b;
        public List<uint> c;
        public List<ulong> d;
        public List<UInt128> e;
        public List<sbyte> f;
        public List<short> g;
        public List<int> h;
        public List<long> i;
        public List<Int128> j;
        public List<bool> k;
        public List<float> l;
        public List<double> m;
        public List<string> n;
        public List<Identity> o;
        public List<Address> p;
    }

    [SpacetimeDB.Table]
    public partial struct OneU8
    {
        public byte n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_u8(byte n)
    {
        new OneU8 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneU16
    {
        public ushort n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_u16(ushort n)
    {
        new OneU16 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneU32
    {
        public uint n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_u32(uint n)
    {
        new OneU32 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneU64
    {
        public ulong n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_u64(ulong n)
    {
        new OneU64 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneU128
    {
        public UInt128 n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_u128(UInt128 n)
    {
        new OneU128 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneI8
    {
        public sbyte n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_i8(sbyte n)
    {
        new OneI8 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneI16
    {
        public short n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_i16(short n)
    {
        new OneI16 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneI32
    {
        public int n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_i32(int n)
    {
        new OneI32 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneI64
    {
        public long n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_i64(long n)
    {
        new OneI64 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneI128
    {
        public Int128 n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_i128(Int128 n)
    {
        new OneI128 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneBool
    {
        public bool b;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_bool(bool b)
    {
        new OneBool { b = b }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneF32
    {
        public float f;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_f32(float f)
    {
        new OneF32 { f = f }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneF64
    {
        public double f;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_f64(double f)
    {
        new OneF64 { f = f }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneString
    {
        public string s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_string(string s)
    {
        new OneString { s = s }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneIdentity
    {
        public Identity i;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_identity(Identity i)
    {
        new OneIdentity { i = i }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneAddress
    {
        public Address a;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_address(Address a)
    {
        new OneAddress { a = a }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneSimpleEnum
    {
        public SimpleEnum e;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_simple_enum(SimpleEnum e)
    {
        new OneSimpleEnum { e = e }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneEnumWithPayload
    {
        public EnumWithPayload e;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_enum_with_payload(EnumWithPayload e)
    {
        new OneEnumWithPayload { e = e }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneUnitStruct
    {
        public UnitStruct s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_unit_struct(UnitStruct s)
    {
        new OneUnitStruct { s = s }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneByteStruct
    {
        public ByteStruct s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_byte_struct(ByteStruct s)
    {
        new OneByteStruct { s = s }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneEveryPrimitiveStruct
    {
        public EveryPrimitiveStruct s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_every_primitive_struct(EveryPrimitiveStruct s)
    {
        new OneEveryPrimitiveStruct { s = s }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct OneEveryVecStruct
    {
        public EveryVecStruct s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_one_every_vec_struct(EveryVecStruct s)
    {
        new OneEveryVecStruct { s = s }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecU8
    {
        public List<byte> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_u8(List<byte> n)
    {
        new VecU8 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecU16
    {
        public List<ushort> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_u16(List<ushort> n)
    {
        new VecU16 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecU32
    {
        public List<uint> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_u32(List<uint> n)
    {
        new VecU32 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecU64
    {
        public List<ulong> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_u64(List<ulong> n)
    {
        new VecU64 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecU128
    {
        public List<UInt128> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_u128(List<UInt128> n)
    {
        new VecU128 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecI8
    {
        public List<sbyte> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_i8(List<sbyte> n)
    {
        new VecI8 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecI16
    {
        public List<short> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_i16(List<short> n)
    {
        new VecI16 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecI32
    {
        public List<int> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_i32(List<int> n)
    {
        new VecI32 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecI64
    {
        public List<long> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_i64(List<long> n)
    {
        new VecI64 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecI128
    {
        public List<Int128> n;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_i128(List<Int128> n)
    {
        new VecI128 { n = n }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecBool
    {
        public List<bool> b;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_bool(List<bool> b)
    {
        new VecBool { b = b }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecF32
    {
        public List<float> f;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_f32(List<float> f)
    {
        new VecF32 { f = f }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecF64
    {
        public List<double> f;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_f64(List<double> f)
    {
        new VecF64 { f = f }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecString
    {
        public List<string> s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_string(List<string> s)
    {
        new VecString { s = s }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecIdentity
    {
        public List<Identity> i;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_identity(List<Identity> i)
    {
        new VecIdentity { i = i }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecAddress
    {
        public List<Address> a;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_address(List<Address> a)
    {
        new VecAddress { a = a }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecSimpleEnum
    {
        public List<SimpleEnum> e;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_simple_enum(List<SimpleEnum> e)
    {
        new VecSimpleEnum { e = e }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecEnumWithPayload
    {
        public List<EnumWithPayload> e;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_enum_with_payload(List<EnumWithPayload> e)
    {
        new VecEnumWithPayload { e = e }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecUnitStruct
    {
        public List<UnitStruct> s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_unit_struct(List<UnitStruct> s)
    {
        new VecUnitStruct { s = s }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecByteStruct
    {
        public List<ByteStruct> s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_byte_struct(List<ByteStruct> s)
    {
        new VecByteStruct { s = s }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecEveryPrimitiveStruct
    {
        public List<EveryPrimitiveStruct> s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_every_primitive_struct(List<EveryPrimitiveStruct> s)
    {
        new VecEveryPrimitiveStruct { s = s }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct VecEveryVecStruct
    {
        public List<EveryVecStruct> s;
    }

    [SpacetimeDB.Reducer]
    public static void insert_vec_every_vec_struct(List<EveryVecStruct> s)
    {
        new VecEveryVecStruct { s = s }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct UniqueU8
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public byte n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_u8(byte n, int data)
    {
        new UniqueU8 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_u8(byte n, int data)
    {
        var key = n;
        UniqueU8.UpdateByn(key, new UniqueU8 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_u8(byte n)
    {
        UniqueU8.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct UniqueU16
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public ushort n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_u16(ushort n, int data)
    {
        new UniqueU16 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_u16(ushort n, int data)
    {
        var key = n;
        UniqueU16.UpdateByn(key, new UniqueU16 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_u16(ushort n)
    {
        UniqueU16.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct UniqueU32
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public uint n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_u32(uint n, int data)
    {
        new UniqueU32 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_u32(uint n, int data)
    {
        var key = n;
        UniqueU32.UpdateByn(key, new UniqueU32 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_u32(uint n)
    {
        UniqueU32.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct UniqueU64
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public ulong n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_u64(ulong n, int data)
    {
        new UniqueU64 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_u64(ulong n, int data)
    {
        var key = n;
        UniqueU64.UpdateByn(key, new UniqueU64 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_u64(ulong n)
    {
        UniqueU64.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct UniqueU128
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public UInt128 n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_u128(UInt128 n, int data)
    {
        new UniqueU128 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_u128(UInt128 n, int data)
    {
        var key = n;
        UniqueU128.UpdateByn(key, new UniqueU128 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_u128(UInt128 n)
    {
        UniqueU128.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct UniqueI8
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public sbyte n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_i8(sbyte n, int data)
    {
        new UniqueI8 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_i8(sbyte n, int data)
    {
        var key = n;
        UniqueI8.UpdateByn(key, new UniqueI8 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_i8(sbyte n)
    {
        UniqueI8.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct UniqueI16
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public short n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_i16(short n, int data)
    {
        new UniqueI16 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_i16(short n, int data)
    {
        var key = n;
        UniqueI16.UpdateByn(key, new UniqueI16 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_i16(short n)
    {
        UniqueI16.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct UniqueI32
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public int n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_i32(int n, int data)
    {
        new UniqueI32 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_i32(int n, int data)
    {
        var key = n;
        UniqueI32.UpdateByn(key, new UniqueI32 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_i32(int n)
    {
        UniqueI32.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct UniqueI64
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public long n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_i64(long n, int data)
    {
        new UniqueI64 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_i64(long n, int data)
    {
        var key = n;
        UniqueI64.UpdateByn(key, new UniqueI64 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_i64(long n)
    {
        UniqueI64.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct UniqueI128
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public Int128 n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_i128(Int128 n, int data)
    {
        new UniqueI128 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_i128(Int128 n, int data)
    {
        var key = n;
        UniqueI128.UpdateByn(key, new UniqueI128 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_i128(Int128 n)
    {
        UniqueI128.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct UniqueBool
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public bool b;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_bool(bool b, int data)
    {
        new UniqueBool { b = b, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_bool(bool b, int data)
    {
        var key = b;
        UniqueBool.UpdateByb(key, new UniqueBool { b = b, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_bool(bool b)
    {
        UniqueBool.DeleteByb(b);
    }

    [SpacetimeDB.Table]
    public partial struct UniqueString
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public string s;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_string(string s, int data)
    {
        new UniqueString { s = s, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_string(string s, int data)
    {
        var key = s;
        UniqueString.UpdateBys(key, new UniqueString { s = s, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_string(string s)
    {
        UniqueString.DeleteBys(s);
    }

    [SpacetimeDB.Table]
    public partial struct UniqueIdentity
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public Identity i;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_identity(Identity i, int data)
    {
        new UniqueIdentity { i = i, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_identity(Identity i, int data)
    {
        var key = i;
        UniqueIdentity.UpdateByi(key, new UniqueIdentity { i = i, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_identity(Identity i)
    {
        UniqueIdentity.DeleteByi(i);
    }

    [SpacetimeDB.Table]
    public partial struct UniqueAddress
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public Address a;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_address(Address a, int data)
    {
        new UniqueAddress { a = a, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_unique_address(Address a, int data)
    {
        var key = a;
        UniqueAddress.UpdateBya(key, new UniqueAddress { a = a, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_address(Address a)
    {
        UniqueAddress.DeleteBya(a);
    }

    [SpacetimeDB.Table]
    public partial struct PkU8
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public byte n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_u8(byte n, int data)
    {
        new PkU8 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_u8(byte n, int data)
    {
        var key = n;
        PkU8.UpdateByn(key, new PkU8 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_u8(byte n)
    {
        PkU8.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct PkU16
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public ushort n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_u16(ushort n, int data)
    {
        new PkU16 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_u16(ushort n, int data)
    {
        var key = n;
        PkU16.UpdateByn(key, new PkU16 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_u16(ushort n)
    {
        PkU16.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct PkU32
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public uint n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_u32(uint n, int data)
    {
        new PkU32 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_u32(uint n, int data)
    {
        var key = n;
        PkU32.UpdateByn(key, new PkU32 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_u32(uint n)
    {
        PkU32.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct PkU64
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public ulong n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_u64(ulong n, int data)
    {
        new PkU64 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_u64(ulong n, int data)
    {
        var key = n;
        PkU64.UpdateByn(key, new PkU64 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_u64(ulong n)
    {
        PkU64.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct PkU128
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public UInt128 n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_u128(UInt128 n, int data)
    {
        new PkU128 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_u128(UInt128 n, int data)
    {
        var key = n;
        PkU128.UpdateByn(key, new PkU128 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_u128(UInt128 n)
    {
        PkU128.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct PkI8
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public sbyte n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_i8(sbyte n, int data)
    {
        new PkI8 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_i8(sbyte n, int data)
    {
        var key = n;
        PkI8.UpdateByn(key, new PkI8 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_i8(sbyte n)
    {
        PkI8.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct PkI16
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public short n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_i16(short n, int data)
    {
        new PkI16 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_i16(short n, int data)
    {
        var key = n;
        PkI16.UpdateByn(key, new PkI16 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_i16(short n)
    {
        PkI16.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct PkI32
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public int n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_i32(int n, int data)
    {
        new PkI32 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_i32(int n, int data)
    {
        var key = n;
        PkI32.UpdateByn(key, new PkI32 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_i32(int n)
    {
        PkI32.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct PkI64
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public long n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_i64(long n, int data)
    {
        new PkI64 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_i64(long n, int data)
    {
        var key = n;
        PkI64.UpdateByn(key, new PkI64 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_i64(long n)
    {
        PkI64.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct PkI128
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public Int128 n;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_i128(Int128 n, int data)
    {
        new PkI128 { n = n, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_i128(Int128 n, int data)
    {
        var key = n;
        PkI128.UpdateByn(key, new PkI128 { n = n, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_i128(Int128 n)
    {
        PkI128.DeleteByn(n);
    }

    [SpacetimeDB.Table]
    public partial struct PkBool
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public bool b;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_bool(bool b, int data)
    {
        new PkBool { b = b, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_bool(bool b, int data)
    {
        var key = b;
        PkBool.UpdateByb(key, new PkBool { b = b, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_bool(bool b)
    {
        PkBool.DeleteByb(b);
    }

    [SpacetimeDB.Table]
    public partial struct PkString
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public string s;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_string(string s, int data)
    {
        new PkString { s = s, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_string(string s, int data)
    {
        var key = s;
        PkString.UpdateBys(key, new PkString { s = s, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_string(string s)
    {
        PkString.DeleteBys(s);
    }

    [SpacetimeDB.Table]
    public partial struct PkIdentity
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public Identity i;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_identity(Identity i, int data)
    {
        new PkIdentity { i = i, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_identity(Identity i, int data)
    {
        var key = i;
        PkIdentity.UpdateByi(key, new PkIdentity { i = i, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_identity(Identity i)
    {
        PkIdentity.DeleteByi(i);
    }

    [SpacetimeDB.Table]
    public partial struct PkAddress
    {
        [SpacetimeDB.Column(ColumnAttrs.PrimaryKey)]
        public Address a;
        public int data;
    }

    [SpacetimeDB.Reducer]
    public static void insert_pk_address(Address a, int data)
    {
        new PkAddress { a = a, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void update_pk_address(Address a, int data)
    {
        var key = a;
        PkAddress.UpdateBya(key, new PkAddress { a = a, data = data });
    }

    [SpacetimeDB.Reducer]
    public static void delete_pk_address(Address a)
    {
        PkAddress.DeleteBya(a);
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_one_identity(DbEventArgs ctx)
    {
        new OneIdentity { i = ctx.Sender }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_vec_identity(DbEventArgs ctx)
    {
        new VecIdentity { i = new List<Identity> { ctx.Sender } }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_unique_identity(DbEventArgs ctx, int data)
    {
        new UniqueIdentity { i = ctx.Sender, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_pk_identity(DbEventArgs ctx, int data)
    {
        new PkIdentity { i = ctx.Sender, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_one_address(DbEventArgs ctx)
    {
        new OneAddress { a = (Address)ctx.Address!, }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_vec_address(DbEventArgs ctx)
    {
        // VecAddress::insert(VecAddress {
        //     < a[_]>::into_vec(
        //         #[rustc_box]
        //         ::alloc::boxed::Box::new([ctx.Address.context("No address in reducer context")?]),
        //     ),
        // });
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_unique_address(DbEventArgs ctx, int data)
    {
        new UniqueAddress { a = (Address)ctx.Address!, data = data }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void insert_caller_pk_address(DbEventArgs ctx, int data)
    {
        new PkAddress { a = (Address)ctx.Address!, data = data }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct LargeTable
    {
        public byte a;
        public ushort b;
        public uint c;
        public ulong d;
        public UInt128 e;
        public sbyte f;
        public short g;
        public int h;
        public long i;
        public Int128 j;
        public bool k;
        public float l;
        public double m;
        public string n;
        public SimpleEnum o;
        public EnumWithPayload p;
        public UnitStruct q;
        public ByteStruct r;
        public EveryPrimitiveStruct s;
        public EveryVecStruct t;
    }

    [SpacetimeDB.Reducer]
    public static void insert_large_table(
        byte a,
        ushort b,
        uint c,
        ulong d,
        UInt128 e,
        sbyte f,
        short g,
        int h,
        long i,
        Int128 j,
        bool k,
        float l,
        double m,
        string n,
        SimpleEnum o,
        EnumWithPayload p,
        UnitStruct q,
        ByteStruct r,
        EveryPrimitiveStruct s,
        EveryVecStruct t
    )
    {
        new LargeTable
        {
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
        }.Insert();
    }

    [SpacetimeDB.Table]
    public partial struct TableHoldsTable
    {
        public OneU8 a;
        public VecU8 b;
    }

    [SpacetimeDB.Reducer]
    public static void insert_table_holds_table(OneU8 a, VecU8 b)
    {
        new TableHoldsTable { a = a, b = b }.Insert();
    }
}
