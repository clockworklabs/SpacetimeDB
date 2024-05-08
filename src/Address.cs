
using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB.SATS;

namespace SpacetimeDB
{
    public struct Address : IEquatable<Address>
    {
        private byte[] bytes;

        public const int SIZE = 16;

        public byte[] Bytes => bytes;

        public static AlgebraicType GetAlgebraicType()
        {
            return new AlgebraicType
            {
                type = AlgebraicType.Type.Builtin,
                builtin = new BuiltinType
                {
                    type = BuiltinType.Type.Array,
                    arrayType = new AlgebraicType
                    {
                        type = AlgebraicType.Type.Builtin,
                        builtin = new BuiltinType
                        {
                            type = BuiltinType.Type.U8
                        }
                    }
                }
            };
        }

        public static explicit operator Address(AlgebraicValue v) => new Address
        {
            bytes = v.AsBytes(),
        };

        public static Address? From(byte[] bytes)
        {
            if (bytes.All(b => b == 0)) {
              return null;
            }
            return new Address
            {
                bytes = bytes,
            };
        }

        public bool Equals(Address other) => ByteArrayComparer.Instance.Equals(bytes, other.bytes);

        public override bool Equals(object o) => o is Address other && Equals(other);

        public static bool operator ==(Address a, Address b) => a.Equals(b);
        public static bool operator !=(Address a, Address b) => !a.Equals(b);

        public static Address Random() {
            Random rnd = new Random();
            var bytes = new byte[16];
            rnd.NextBytes(bytes);
            return new Address{ bytes = bytes, };
        }

        public override int GetHashCode() => ByteArrayComparer.Instance.GetHashCode(bytes);

        public override string ToString() => ByteArrayComparer.ToHexString(bytes);
    }
}
