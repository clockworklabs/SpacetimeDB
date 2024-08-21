using System;
using System.IO;
using System.Linq;
using SpacetimeDB.BSATN;

namespace SpacetimeDB
{
    public readonly struct Address : IEquatable<Address>
    {
        public const int SIZE = 16;

        public readonly byte[] Bytes;

        private Address(byte[] bytes) => Bytes = bytes;

        public readonly struct BSATN : IReadWrite<Address>
        {
            public Address Read(BinaryReader reader) =>
                new(ByteArray.Instance.Read(reader));

            public void Write(BinaryWriter writer, Address value) =>
                ByteArray.Instance.Write(writer, value.Bytes);

            public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
                new AlgebraicType.Product(
                    new AggregateElement[]
                    {
                        new("__address_bytes", ByteArray.Instance.GetAlgebraicType(registrar))
                    }
                );
        }

        public static Address? From(byte[] bytes)
        {
            if (bytes.All(b => b == 0))
            {
                return null;
            }
            return new(bytes);
        }

        public bool Equals(Address other) => ByteArrayComparer.Instance.Equals(Bytes, other.Bytes);

        public override bool Equals(object? o) => o is Address other && Equals(other);

        public static bool operator ==(Address a, Address b) => a.Equals(b);
        public static bool operator !=(Address a, Address b) => !a.Equals(b);

        public static Address Random()
        {
            var random = new Random();
            var bytes = new byte[16];
            random.NextBytes(bytes);
            return new(bytes);
        }

        public override int GetHashCode() => ByteArrayComparer.Instance.GetHashCode(Bytes);

        public override string ToString() => ByteArrayComparer.ToHexString(Bytes);
    }
}
