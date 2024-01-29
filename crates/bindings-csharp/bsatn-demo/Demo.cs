// using SpacetimeDB.Module;
// using static SpacetimeDB.Runtime;

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
    public partial record EnumWithPayload
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
            // Identity Identity,
            // Address Address,
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
        // public Identity o;
        // public Address p;
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
        // public List<Identity> o;
        // public List<Address> p;
    }

    [SpacetimeDB.Type]
    public partial struct Foo<T, TRW>
        where TRW : SpacetimeDB.BSATN.IReadWrite<T>
    {
        public T a;
        public T b;
    }
}
