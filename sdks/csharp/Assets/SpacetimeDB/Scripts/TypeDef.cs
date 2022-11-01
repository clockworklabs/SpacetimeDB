using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using Google.Protobuf;
using UnityEngine;

namespace SpacetimeDB
{
    public class TypeDef
    {
        public enum Def
        {
            Tuple = 0,
            Enum,

            // base types
            Vec,
            U8,
            U16,
            U32,
            U64,
            U128,
            I8,
            I16,
            I32,
            I64,
            I128,
            Bool,
            F32,
            F64,
            String,
            Bytes,
            Hash,
            Unit,
        }

        public Def Type => type;
        public ElementDef[] TupleElements => tupleElements;

        public TypeDef VecMemberType => vecMemberType;

        public Def type;
        private TypeDef vecMemberType;
        public ElementDef[] tupleElements;

        public static TypeDef GetVec(TypeDef memberType)
        {
            return new TypeDef
            {
                type = Def.Vec,
                vecMemberType = memberType,
            };
        }

        public static TypeDef BuiltInType(Def def)
        {
            switch (def)
            {
                case Def.Tuple:
                case Def.Enum:
                case Def.Vec:
                    Debug.LogError("This is not a built-in type");
                    break;
            }

            return new TypeDef
            {
                type = def,
            };
        }

        public static TypeDef Tuple(ElementDef[] elements)
        {
            return new TypeDef
            {
                type = Def.Tuple,
                tupleElements = elements,
            };
        }
    }

    public struct ElementDef
    {
        public byte tag;
        public TypeDef element;

        public ElementDef(byte tag, TypeDef element)
        {
            this.tag = tag;
            this.element = element;
        }
    }

    public struct TypeValue
    {
        public class TypeValueComparer : IEqualityComparer<TypeValue>
        {
            public bool Equals(TypeValue value1, TypeValue value2)
            {
                if (value1.typeDef.Type != value2.typeDef.Type)
                {
                    return false;
                }

                // Handle any strange cases
                switch (value1.typeDef.Type)
                {
                    case TypeDef.Def.Tuple:
                        if ((value1.tupleElements == null) != (value2.tupleElements == null))
                        {
                            return false;
                        }

                        if (value1.tupleElements == null)
                        {
                            break;
                        }

                        if (value1.tupleElements.Length != value2.tupleElements.Length)
                        {
                            return false;
                        }

                        break;
                }

                return true;
            }

            public int GetHashCode(TypeValue obj)
            {
                switch (obj.typeDef.Type)
                {
                    case TypeDef.Def.Bool:
                        return (int)(obj.b ? 0xab745abf : 0xf60d5833);
                    case TypeDef.Def.Bytes:
                        if (obj.bytes == null)
                        {
                            throw new InvalidOperationException("Cannot hash on null bytes.");
                        }

                        return (int)(obj.bytes.GetHashCode() ^ 0x8595a70b);
                    case TypeDef.Def.Enum:
                        throw new NotImplementedException();
                    case TypeDef.Def.F32:
                    case TypeDef.Def.F64:
                        throw new InvalidOperationException("Cannot hash on floats");
                    case TypeDef.Def.Hash:
                        return (int)(obj.hash.GetHashCode() ^ 0x8595a70b);
                    case TypeDef.Def.I8:
                        return (int)(obj.signed ^ 0x6bac6c4e);
                    case TypeDef.Def.I16:
                        return (int)(obj.signed ^ 0x2b4e01bf);
                    case TypeDef.Def.I32:
                        return (int)(obj.signed ^ 0xe32a7812);
                    case TypeDef.Def.I64:
                        return (int)(obj.signed ^ 0xab745abf ^ (obj.signed >> 32));
                    case TypeDef.Def.U8:
                        return (int)(obj.unsigned ^ 0xc2964a29);
                    case TypeDef.Def.U16:
                        return (int)(obj.unsigned ^ 0xfb504320);
                    case TypeDef.Def.U32:
                        return (int)(obj.unsigned ^ 0x74208545);
                    case TypeDef.Def.U64:
                        return (int)(obj.unsigned ^ 0x3ef90ce5 ^ (obj.unsigned >> 32));
                    case TypeDef.Def.I128:
                    case TypeDef.Def.U128:
                        throw new InvalidOperationException("Hashing on 128 bit integers unsupported");
                    case TypeDef.Def.String:
                        if (obj.str == null)
                        {
                            throw new InvalidOperationException("Cannot hash on null string.");
                        }

                        return obj.str.GetHashCode() ^ 0x33e1b1f4;
                    case TypeDef.Def.Tuple:
                    {
                        var hash = (int)(obj.tupleElements.Length ^ 0xbbce67bf);
                        foreach (var element in obj.tupleElements)
                        {
                            hash ^= element.GetHashCode();
                        }

                        return hash;
                    }
                    case TypeDef.Def.Unit:
                        return 0x64b22bc4;
                    case TypeDef.Def.Vec:
                        var vecHash = 0x0fec85d5;
                        foreach (var value in obj.vec)
                        {
                            vecHash ^= value.GetHashCode();
                        }

                        return vecHash;
                }

                throw new NotImplementedException($"Hashing on: {obj.typeDef.Type}");
            }
        }

        public TypeDef TypeDef => typeDef;

        private TypeDef typeDef;
        private ulong unsigned;
        private long signed;
        private string str;
        private byte[] bytes;
        private Hash hash;
        private bool b;

        private float f32;
        private double f64;
        private TypeValue[] tupleElements;
        private List<TypeValue> vec;

        public override string ToString()
        {
            if (tupleElements != null) {
                var s = "(\n";
                foreach (var e in tupleElements) {
                    s += "\t";
                    s += e.ToString();
                    s += "\n";
                }
                s += ")";
                return s;
            } else {
                return String.Format($"typeDef={typeDef.Type}, unsigned={unsigned}, signed={signed}, str={str}, bytes={bytes}, bool={b}, float={f32}, double={f64}");
            }
        }

        public static (TypeValue?, int) Decode(TypeDef def, ByteString bytes)
        {
            var b = bytes[1];
            var byteArr = bytes.ToByteArray();
            return Decode(def, byteArr, 0, byteArr.Length);
        }

        public static (TypeValue?, int) Decode(TypeDef def, byte[] arr, int offset, int length)
        {
            var value = new TypeValue
            {
                typeDef = def
            };
            var read = 0;

            try
            {
                switch (def.Type)
                {
                    case TypeDef.Def.Bool:
                        value.b = arr[offset] != 0;
                        read = 1;
                        break;
                    case TypeDef.Def.U8:
                        value.unsigned = arr[offset];
                        read = 1;
                        break;
                    case TypeDef.Def.U16:
                        value.unsigned = BitConverter.ToUInt16(arr, offset);
                        read = 2;
                        break;
                    case TypeDef.Def.U32:
                        value.unsigned = BitConverter.ToUInt32(arr, offset);
                        read = 4;
                        break;
                    case TypeDef.Def.U64:
                        value.unsigned = BitConverter.ToUInt64(arr, offset);
                        read = 8;
                        break;
                    case TypeDef.Def.I8:
                        value.signed = arr[offset];
                        read = 1;
                        break;
                    case TypeDef.Def.I16:
                        value.signed = BitConverter.ToInt16(arr, offset);
                        read = 2;
                        break;
                    case TypeDef.Def.I32:
                        value.signed = BitConverter.ToInt32(arr, offset);
                        read = 4;
                        break;
                    case TypeDef.Def.I64:
                        value.signed = BitConverter.ToInt64(arr, offset);
                        read = 8;
                        break;
                    case TypeDef.Def.F32:
                        value.f32 = BitConverter.ToSingle(arr, offset);
                        read = 4;
                        break;
                    case TypeDef.Def.F64:
                        value.f64 = BitConverter.ToDouble(arr, offset);
                        read = 8;
                        break;
                    case TypeDef.Def.String:
                        var strLength = BitConverter.ToUInt16(arr, offset);
                        value.str = Encoding.UTF8.GetString(arr, offset + 2, strLength);
                        read += strLength + 2;
                        break;
                    case TypeDef.Def.Bytes:
                        var byteLength = BitConverter.ToUInt16(arr, offset);
                        if (byteLength >= arr.Length - (offset + 2))
                        {
                            throw new InvalidOperationException(
                                $"Read error: byte array goes past the end of the array: {byteLength}");
                        }

                        value.bytes = new byte[byteLength];
                        Array.Copy(arr, offset + 2, value.bytes, 0, byteLength);
                        read += byteLength + 2;
                        break;
                    case TypeDef.Def.Hash:
                        byte[] hashVal = new byte[Hash.SIZE];
                        Array.Copy(arr, offset, hashVal, 0, Hash.SIZE);
                        read += Hash.SIZE;
                        value.hash = Hash.From(hashVal);
                        break;
                    case TypeDef.Def.Tuple:
                        return ReadTuple(def, arr, offset, length);
                    case TypeDef.Def.Vec:
                        if (def.VecMemberType == null)
                        {
                            throw new InvalidOperationException("Read error: vec has no member type!");
                        }

                        var vecLength = BitConverter.ToUInt16(arr, offset);
                        read += 2;
                        value.vec = new List<TypeValue>();
                        for (var idx = 0; idx < vecLength; idx++)
                        {
                            var (entry, subDecodeRead) = Decode(def.VecMemberType, arr, offset + read, length);
                            read += subDecodeRead;
                            if (entry.HasValue)
                            {
                                value.vec.Add(entry.Value);
                            }
                        }

                        break;
                    default:
                        Debug.LogError($"This type is unsupported for now: {def.Type}");
                        return (null, 0);
                }
            }
            catch (Exception e)
            {
                Debug.LogException(e);
                Debug.LogError("Are your table definitions in BitCraftMiniGameManager correct?");
                return (null, 0);
            }

            return (value, read);
        }

        private static (TypeValue?, int) ReadTuple(TypeDef def, byte[] arr, int offset, int length)
        {
            var read = 0;
            var resultElements = new TypeValue[def.TupleElements.Length];
            var elementIdx = 0;
            foreach (var elementDef in def.TupleElements)
            {
                var (value, bytesRead) = Decode(elementDef.element, arr, offset + read, length);

                if (!value.HasValue)
                {
                    return (null, 0);
                }

                read += bytesRead;
                resultElements[elementIdx++] = value.Value;
            }

            return (GetTuple(def, resultElements), read);
        }

        public object GetValue(TypeDef.Def def)
        {
            switch (def)
            {
                case TypeDef.Def.Bool:
                    return b;
                case TypeDef.Def.U8:
                    return (byte)unsigned;
                case TypeDef.Def.U16:
                    return (ushort)unsigned;
                case TypeDef.Def.U32:
                    return (uint)unsigned;
                case TypeDef.Def.U64:
                    return unsigned;
                case TypeDef.Def.U128:
                    throw new InvalidOperationException("U128 not supported in C#");

                case TypeDef.Def.I8:
                    return (byte)signed;
                case TypeDef.Def.I16:
                    return (ushort)signed;
                case TypeDef.Def.I32:
                    return (int)signed;
                case TypeDef.Def.I64:
                    return signed;
                case TypeDef.Def.I128:
                    throw new InvalidOperationException("I128 not supported in C#");

                case TypeDef.Def.String:
                    return str;
                case TypeDef.Def.Bytes:
                    if (bytes == null)
                        throw new InvalidOperationException("byte array is null!");
                    return bytes;
                case TypeDef.Def.Hash:
                    return hash;
                case TypeDef.Def.F32:
                    return f32;
                case TypeDef.Def.F64:
                    return f64;
                case TypeDef.Def.Tuple:
                    return tupleElements;
                case TypeDef.Def.Vec:
                    return vec;
            }

            throw new InvalidOperationException($"Type not supported yet! {def}");
        }

        public object GetValue(TypeDef def)
        {
            return GetValue(def.Type);
        }

        public static TypeValue GetTuple(TypeDef def, TypeValue[] tupleValues)
        {
            return new TypeValue
            {
                typeDef = def,
                tupleElements = tupleValues,
            };
        }

        public static TypeValue GetVec(TypeDef memberType, IEnumerable<TypeValue> values)
        {
            return new TypeValue
            {
                typeDef = TypeDef.GetVec(memberType),
                vec = new List<TypeValue>(values),
            };
        }
    }

    public struct Hash : IEquatable<Hash>
    {
        private byte[] bytes;

        public static int SIZE = 32;

        public byte[] Bytes => bytes;

        public static TypeDef GetTypeDef()
        {
            return TypeDef.BuiltInType(TypeDef.Def.Hash);
        }

        public static explicit operator Hash(TypeValue v) => (Hash)v.GetValue(GetTypeDef());

        public static Hash From(byte[] bytes)
        {
            // TODO: should we validate length here?
            return new Hash
            {
                bytes = bytes,
            };
        }

        public bool Equals(Hash other)
        {
            return bytes.SequenceEqual(other.bytes);
        }

        public override bool Equals(object o) {
            return o is Hash other && Equals(other);
        }

        public static bool operator ==(Hash a, Hash b) => a.Equals(b);
        public static bool operator !=(Hash a, Hash b) => !a.Equals(b);

        public override int GetHashCode() {
            if (bytes == null)
            {
                throw new InvalidOperationException("Cannot hash on null bytes.");
            }

            return bytes.GetHashCode();
        }
    }
}
