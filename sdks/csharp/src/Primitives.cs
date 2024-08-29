using SpacetimeDB.BSATN;

using System;
using System.IO;

namespace SpacetimeDB
{

    public struct I128 : IEquatable<I128>
    {
        public long hi;
        public ulong lo;

        public I128(long hi, ulong lo)
        {
            this.hi = hi;
            this.lo = lo;
        }

        public readonly bool Equals(I128 x) => hi == x.hi && lo == x.lo;

        public override readonly bool Equals(object? o) => o is I128 x && Equals(x);

        public static bool operator ==(I128 a, I128 b) => a.Equals(b);
        public static bool operator !=(I128 a, I128 b) => !a.Equals(b);

        public override readonly int GetHashCode() => hi.GetHashCode() ^ lo.GetHashCode();

        public override readonly string ToString() => $"I128({hi},{lo})";

        public readonly struct BSATN : IReadWrite<I128>
        {
            public I128 Read(BinaryReader reader) => new(reader.ReadInt64(), reader.ReadUInt64());

            public void Write(BinaryWriter writer, I128 value)
            {
                writer.Write(value.hi);
                writer.Write(value.lo);
            }

            public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
                new AlgebraicType.Builtin(new BuiltinType.I128(new Unit()));
        }
    }

    public struct U128 : IEquatable<U128>
    {
        public ulong hi;
        public ulong lo;

        public U128(ulong hi, ulong lo)
        {
            this.lo = lo;
            this.hi = hi;
        }

        public readonly bool Equals(U128 x) => hi == x.hi && lo == x.lo;

        public override readonly bool Equals(object? o) => o is U128 x && Equals(x);

        public static bool operator ==(U128 a, U128 b) => a.Equals(b);
        public static bool operator !=(U128 a, U128 b) => !a.Equals(b);

        public override readonly int GetHashCode() => hi.GetHashCode() ^ lo.GetHashCode();

        public override readonly string ToString() => $"U128({hi},{lo})";

        public readonly struct BSATN : IReadWrite<U128>
        {
            public U128 Read(BinaryReader reader) => new(reader.ReadUInt64(), reader.ReadUInt64());

            public void Write(BinaryWriter writer, U128 value)
            {
                writer.Write(value.hi);
                writer.Write(value.lo);
            }

            public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
                new AlgebraicType.Builtin(new BuiltinType.U128(new Unit()));
        }
    }

}
