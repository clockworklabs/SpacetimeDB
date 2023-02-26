using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using SpacetimeDB.SATS;
using UnityEngine;

namespace SpacetimeDB
{
    public struct Identity : IEquatable<Identity>
    {
        private byte[] bytes;

        public static int SIZE = 32;

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

        public static explicit operator Identity(AlgebraicValue v) => new Identity
        {
            bytes = v.AsBytes(),
        };

        public static Identity From(byte[] bytes)
        {
            // TODO: should we validate length here?
            return new Identity
            {
                bytes = bytes,
            };
        }

        public bool Equals(Identity other)
        {
            return bytes.SequenceEqual(other.bytes);
        }

        public override bool Equals(object o)
        {
            return o is Identity other && Equals(other);
        }

        public static bool operator ==(Identity a, Identity b) => a.Equals(b);
        public static bool operator !=(Identity a, Identity b) => !a.Equals(b);

        public override int GetHashCode()
        {
            if (bytes == null)
            {
                throw new InvalidOperationException("Cannot hash on null bytes.");
            }

            return bytes.GetHashCode();
        }
    }
}
