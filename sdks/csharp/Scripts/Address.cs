
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

        public static int SIZE = 16;

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

        public bool Equals(Address other)
        {
            return bytes.SequenceEqual(other.bytes);
        }

        public override bool Equals(object o)
        {
            return o is Address other && Equals(other);
        }

        public static bool operator ==(Address a, Address b) => a.Equals(b);
        public static bool operator !=(Address a, Address b) => !a.Equals(b);

        public static Address Random() {
            Random rnd = new Random();
            var bytes = new byte[16];
            rnd.NextBytes(bytes);
            return new Address{ bytes = bytes, };
        }

        public override int GetHashCode()
        {
            if (bytes == null)
            {
                throw new InvalidOperationException("Cannot hash on null bytes.");
            }

            return BitConverter.ToInt32(bytes, 0);
        }

        public override string ToString()
        {
            return string.Concat(bytes.Select(b => b.ToString("x2")));
        }
    }
}
