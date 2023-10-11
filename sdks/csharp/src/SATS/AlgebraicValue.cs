using System;
using System.Linq;
using System.Collections.Generic;
using System.IO;

namespace SpacetimeDB.SATS
{
    public class SumValue
    {
        public byte tag;
        public AlgebraicValue value;

        private SumValue()
        {
        }

        public SumValue(byte tag, AlgebraicValue value)
        {
            this.tag = tag;
            this.value = value;
        }

        public static SumValue Deserialize(SumType type, BinaryReader reader)
        {
            var result = new SumValue();
            result.tag = reader.ReadByte();
            result.value = AlgebraicValue.Deserialize(type.variants[result.tag].algebraicType, reader);
            return result;
        }

        public void Serialize(SumType type, BinaryWriter writer)
        {
            writer.Write(tag);
            value.Serialize(type.variants[tag].algebraicType, writer);
        }

        public static bool Compare(SumType t, SumValue v1, SumValue v2)
        {
            if (v1.tag != v2.tag)
            {
                return false;
            }

            return AlgebraicValue.Compare(t.variants[v1.tag].algebraicType, v1.value, v2.value);
        }
    }

    public class ProductValue
    {
        public List<AlgebraicValue> elements;

        public ProductValue()
        {
            elements = new List<AlgebraicValue>();
        }

        public void Serialize(ProductType type, BinaryWriter writer)
        {
            writer.Write((uint)elements.Count);
            for (var x = 0; x < elements.Count; x++)
            {
                elements[x].Serialize(type.elements[x].algebraicType, writer);
            }
        }

        public static ProductValue Deserialize(ProductType type, BinaryReader reader)
        {
            var result = new ProductValue();
            for (var x = 0; x < type.elements.Count; x++)
            {
                result.elements.Add(AlgebraicValue.Deserialize(type.elements[x].algebraicType, reader));
            }

            return result;
        }

        public static bool Compare(ProductType type, ProductValue v1, ProductValue v2)
        {
            if (v1.elements.Count != v2.elements.Count)
            {
                return false;
            }

            for (var i = 0; i < type.elements.Count; i++)
            {
                if (!AlgebraicValue.Compare(type.elements[i].algebraicType, v1.elements[i], v2.elements[i]))
                {
                    return false;
                }
            }

            return true;
        }
    }

    public class AlgebraicValue
    {
        private object value;

        public SumValue AsSumValue() => (SumValue)value;
        public ProductValue AsProductValue() => (ProductValue)value;
        public List<AlgebraicValue> AsArray() => (List<AlgebraicValue>)value;
        public SortedDictionary<AlgebraicValue, AlgebraicValue> AsMap() => (SortedDictionary<AlgebraicValue, AlgebraicValue>)value;
        public bool AsBool() => (bool)value;
        public sbyte AsI8() => (sbyte)value;
        public byte AsU8() => (byte)value;
        public short AsI16() => (short)value;
        public ushort AsU16() => (ushort)value;
        public int AsI32() => (int)value;
        public uint AsU32() => (uint)value;
        public long AsI64() => (long)value;
        public ulong AsU64() => (ulong)value;
        public byte[] AsI128() => (byte[])value;
        public byte[] AsU128() => (byte[])value;
        public float AsF32() => (float)value;
        public double AsF64() => (double)value;
        public string AsString() => (string)value;
        public byte[] AsBytes() => (byte[])value;

        public static AlgebraicValue CreateProduct(ProductValue value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateSum(SumValue value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateArray(List<AlgebraicValue> value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateMap(SortedDictionary<AlgebraicValue, AlgebraicValue> value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateBool(bool value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateI8(sbyte value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateU8(byte value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateI16(short value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateU16(ushort value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateI32(int value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateU32(uint value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateI64(long value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateU64(ulong value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateI128(byte[] value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateU128(byte[] value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateF32(float value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateF64(double value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateString(string value) => new AlgebraicValue { value = value };
        public static AlgebraicValue CreateBytes(byte[] value) => new AlgebraicValue { value = value };

        private static bool compareBytes(byte[] arr1, byte[] arr2)
        {
            if (arr1.Length != arr2.Length)
            {
                return false;
            }

            for (var i = 0; i < arr1.Length; i++)
            {
                if (arr1[i] != arr2[i])
                {
                    return false;
                }
            }

            return true;
        }

        public static bool Compare(AlgebraicType t, AlgebraicValue v1, AlgebraicValue v2)
        {
            switch (t.type)
            {
                case AlgebraicType.Type.Sum:
                    return SumValue.Compare(t.sum, v1.AsSumValue(), v2.AsSumValue());
                case AlgebraicType.Type.Product:
                    return ProductValue.Compare(t.product, v1.AsProductValue(), v2.AsProductValue());
                case AlgebraicType.Type.Array:
                    // Fast path for byte arrays.
                    if (t.arrayType.type == AlgebraicType.Type.U8)
                    {
                        return AlgebraicValue.compareBytes(v1.AsBytes(), v2.AsBytes());
                    }

                    var list1 = v1.AsArray();
                    var list2 = v2.AsArray();
                    if (list1.Count != list2.Count)
                    {
                        return false;
                    }

                    for (var i = 0; i < list1.Count; i++)
                    {
                        if (!AlgebraicValue.Compare(t.arrayType, list1[i], list2[i]))
                        {
                            return false;
                        }
                    }
                    return true;
                case AlgebraicType.Type.Map:
                    var dict1 = v1.AsMap();
                    var dict2 = v2.AsMap();
                    // First a fast length check and then ensure that
                    // for every key in the first dict there's a key in the second
                    // where their values match.
                    return dict1.Count == dict2.Count && dict1.All(
                        (dict1KV) => d2.TryGetValue(dict1KV.Key, out var dict2Value) &&
                            AlgebraicValue.Compare(t.valueType, dict1KV.Value, dict2Value)
                    );
                case AlgebraicType.Type.Bool:
                    return v1.AsBool() == v2.AsBool();
                case AlgebraicType.Type.U8:
                    return v1.AsU8() == v2.AsU8();
                case AlgebraicType.Type.I8:
                    return v1.AsI8() == v2.AsI8();
                case AlgebraicType.Type.U16:
                    return v1.AsU16() == v2.AsU16();
                case AlgebraicType.Type.I16:
                    return v1.AsI16() == v2.AsI16();
                case AlgebraicType.Type.U32:
                    return v1.AsU32() == v2.AsU32();
                case AlgebraicType.Type.I32:
                    return v1.AsI32() == v2.AsI32();
                case AlgebraicType.Type.U64:
                    return v1.AsU64() == v2.AsU64();
                case AlgebraicType.Type.I64:
                    return v1.AsI64() == v2.AsI64();
                case AlgebraicType.Type.U128:
                    return AlgebraicValue.compareBytes(v1.AsU128(), v2.AsU128());
                case AlgebraicType.Type.I128:
                    return AlgebraicValue.compareBytes(v1.AsI128(), v2.AsI128());
                // For floats, match the semantics of Rust in not accounting for epsilon.
                case AlgebraicType.Type.F32:
                    return v1.AsF32() == v2.AsF32();
                case AlgebraicType.Type.F64:
                    return v1.AsF64() == v2.AsF64();
                case AlgebraicType.Type.String:
                    return v1.AsString() == v2.AsString();
                case AlgebraicType.Type.TypeRef:
                case AlgebraicType.Type.None:
                default:
                    throw new NotImplementedException();
            }
        }

        public static AlgebraicValue Deserialize(AlgebraicType type, BinaryReader reader)
        {
            switch (type.type)
            {
                case AlgebraicType.Type.Sum:
                    return Create(SumValue.Deserialize(type.sum, reader));
                case AlgebraicType.Type.Product:
                    return Create(ProductValue.Deserialize(type.product, reader));
                case AlgebraicType.Type.Array:
                    if (type.arrayType.type == AlgebraicType.Type.U8)
                    {
                        return CreateBytes(ReadByteArray());
                    }

                    var length = reader.ReadInt32();
                    var arrayResult = new List<AlgebraicValue>();
                    for (var x = 0; x < length; x++)
                    {
                        arrayResult.Add(AlgebraicValue.Deserialize(type.arrayType, reader));
                    }

                    return CreateArray(arrayResult);
                case AlgebraicType.Type.Map:
                    {
                        var len = reader.ReadUInt32();
                        var mapResult = new SortedDictionary<AlgebraicValue, AlgebraicValue>();
                        for (var x = 0; x < len; x++)
                        {
                            var key = AlgebraicValue.Deserialize(type.mapType.keyType, reader);
                            var value = AlgebraicValue.Deserialize(type.mapType.valueType, reader);
                            mapResult.Add(key, value);
                        }

                        return CreateMap(mapResult);
                    }
                case AlgebraicType.Type.Bool:
                    return CreateBool(reader.ReadByte() != 0);
                case AlgebraicType.Type.I8:
                    return CreateI8(reader.ReadSByte());
                case AlgebraicType.Type.U8:
                    return CreateU8(reader.ReadByte());
                case AlgebraicType.Type.I16:
                    return CreateI16(reader.ReadInt16());
                case AlgebraicType.Type.U16:
                    return CreateU16(reader.ReadUInt16());
                case AlgebraicType.Type.I32:
                    return CreateI32(reader.ReadInt32());
                case AlgebraicType.Type.U32:
                    return CreateU32(reader.ReadUInt32());
                case AlgebraicType.Type.I64:
                    return CreateI64(reader.ReadInt64());
                case AlgebraicType.Type.U64:
                    return CreateU64(reader.ReadUInt64());
                case AlgebraicType.Type.I128:
                    return CreateI128(reader.ReadBytes(16));
                case AlgebraicType.Type.U128:
                    return CreateU128(reader.ReadBytes(16));
                case AlgebraicType.Type.F32:
                    return CreateF32(reader.ReadSingle());
                case AlgebraicType.Type.F64:
                    return CreateF64(reader.ReadDouble());
                case AlgebraicType.Type.String:
                    return CreateString(System.Text.Encoding.UTF8.GetString(ReadByteArray()));
                default:
                    throw new NotImplementedException();
            }

            byte[] ReadByteArray()
            {
                var len = reader.ReadUInt32();
                if (len > int.MaxValue)
                {
                    var arrays = new List<byte[]>();
                    long read = 0;
                    while (read < len)
                    {
                        var remaining = len - read;
                        var readResult = reader.ReadBytes(remaining > int.MaxValue ? int.MaxValue : (int)remaining);
                        arrays.Add(readResult);
                        read += readResult.Length;
                    }

                    var result = new byte[len];
                    long pos = 0;
                    foreach (var array in arrays)
                    {
                        Array.Copy(array, 0, result, pos, array.LongLength);
                        pos += array.LongLength;
                    }

                    return result;
                }

                return reader.ReadBytes((int)len);
            }
        }

        public void Serialize(AlgebraicType type, BinaryWriter writer)
        {
            switch (type.type)
            {
                case AlgebraicType.Type.Sum:
                    AsSumValue().Serialize(type.sum, writer);
                    break;
                case AlgebraicType.Type.Product:
                    AsProductValue().Serialize(type.product, writer);
                    break;
                case AlgebraicType.Type.Array:
                    if (type.arrayType.type == AlgebraicType.Type.U8)
                    {
                        WriteByteBuffer(AsBytes());
                        break;
                    }

                    var array = AsArray();
                    writer.Write(array.Count);
                    foreach (var entry in array)
                    {
                        entry.Serialize(type.arrayType, writer);
                    }
                    break;
                case AlgebraicType.Type.Map:
                    // The map is sorted by key, just like `BTreeMap` in Rust
                    // so we can serialize deterministically.
                    var map = AsMap();
                    writer.Write(map.Count);
                    foreach( KeyValuePair<AlgebraicValue, AlgebraicValue> kv in map )
                    {
                        kv.Key.Serialize(type.keyType, writer);
                        kv.Value.Serialize(type.valueType, writer);
                    }
                    break;
                case AlgebraicType.Type.Bool:
                    writer.Write(AsBool());
                    break;
                case AlgebraicType.Type.I8:
                    writer.Write(AsI8());
                    break;
                case AlgebraicType.Type.U8:
                    writer.Write(AsU8());
                    break;
                case AlgebraicType.Type.I16:
                    writer.Write(AsI16());
                    break;
                case AlgebraicType.Type.U16:
                    writer.Write(AsU16());
                    break;
                case AlgebraicType.Type.I32:
                    writer.Write(AsI32());
                    break;
                case AlgebraicType.Type.U32:
                    writer.Write(AsU32());
                    break;
                case AlgebraicType.Type.I64:
                    writer.Write(AsI64());
                    break;
                case AlgebraicType.Type.U64:
                    writer.Write(AsU64());
                    break;
                case AlgebraicType.Type.I128:
                    writer.Write(AsI128());
                    break;
                case AlgebraicType.Type.U128:
                    writer.Write(AsU128());
                    break;
                case AlgebraicType.Type.F32:
                    writer.Write(AsF32());
                    break;
                case AlgebraicType.Type.F64:
                    writer.Write(AsF64());
                    break;
                case AlgebraicType.Type.String:
                    WriteByteBuffer(System.Text.Encoding.UTF8.GetBytes((string)value));
                    break;
                default:
                    throw new NotImplementedException();
            }

            void WriteByteBuffer(byte[] buf)
            {
                if (buf.LongLength > uint.MaxValue)
                {
                    throw new Exception("Serializing a buffer that is too large for SATS.");
                }

                writer.Write((uint)buf.LongLength);
                writer.Write(buf);
            }
        }

        public class AlgebraicValueComparer : IEqualityComparer<AlgebraicValue>
        {
            private AlgebraicType type;
            public AlgebraicValueComparer(AlgebraicType type)
            {
                this.type = type;
            }

            public bool Equals(AlgebraicValue l, AlgebraicValue r)
            {
                return AlgebraicValue.Compare(type, l, r);
            }

            public int GetHashCode(AlgebraicValue value)
            {
                var stream = new MemoryStream();
                var writer = new BinaryWriter(stream);
                value.Serialize(type, writer);
                var s = stream.ToArray();
                if (s.Length >= 4)
                {
                    return BitConverter.ToInt32(s, 0);
                }
                return s.Sum(b => b);
            }
        }

    }
}
