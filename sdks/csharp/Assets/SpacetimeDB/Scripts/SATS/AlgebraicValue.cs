using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using Google.Protobuf.WellKnownTypes;
using TMPro;
using UnityEditor.Build.Content;
using UnityEngine;
using Type = System.Type;

namespace SpacetimeDB.SATS
{
    [StructLayout(LayoutKind.Explicit)]
    public struct BuiltinValue
    {
        [FieldOffset(0)] public bool b;
        [FieldOffset(0)] public sbyte i8;
        [FieldOffset(0)] public byte u8;
        [FieldOffset(0)] public short i16;
        [FieldOffset(0)] public ushort u16;
        [FieldOffset(0)] public int i32;
        [FieldOffset(0)] public uint u32;
        [FieldOffset(0)] public long i64;
        [FieldOffset(0)] public ulong u64;
        [FieldOffset(0)] public byte[] i128;
        [FieldOffset(0)] public byte[] u128;
        [FieldOffset(0)] public float f32;
        [FieldOffset(0)] public double f64;
        [FieldOffset(0)] public string s;
        [FieldOffset(0)] public byte[] bytes;
        [FieldOffset(0)] public List<AlgebraicValue> array;
        [FieldOffset(0)] public Dictionary<AlgebraicValue, AlgebraicValue> map;

        public static BuiltinValue Create(bool b) => new BuiltinValue { b = b };
        public static BuiltinValue Create(sbyte i8) => new BuiltinValue { i8 = i8 };
        public static BuiltinValue Create(byte u8) => new BuiltinValue { u8 = u8 };
        public static BuiltinValue Create(short i16) => new BuiltinValue { i16 = i16 };
        public static BuiltinValue Create(ushort u16) => new BuiltinValue { u16 = u16 };
        public static BuiltinValue Create(int i32) => new BuiltinValue { i32 = i32 };
        public static BuiltinValue Create(uint u32) => new BuiltinValue { u32 = u32 };
        public static BuiltinValue Create(long i64) => new BuiltinValue { i64 = i64 };

        public static BuiltinValue Create(ulong u64) => new BuiltinValue { u64 = u64 };

        public static BuiltinValue CreateI128(byte[] i128) => new BuiltinValue { i128 = i128 };
        public static BuiltinValue CreateU128(byte[] u128) => new BuiltinValue { u128 = u128 };
        public static BuiltinValue Create(float f32) => new BuiltinValue { f32 = f32 };
        public static BuiltinValue Create(double f64) => new BuiltinValue { f64 = f64 };
        public static BuiltinValue Create(string s) => new BuiltinValue { s = s };
        public static BuiltinValue CreateBytes(byte[] bytes) => new BuiltinValue { bytes = bytes };
        public static BuiltinValue Create(List<AlgebraicValue> array) => new BuiltinValue { array = array };
        public static BuiltinValue Create(Dictionary<AlgebraicValue, AlgebraicValue> map) => new BuiltinValue { map = map };

        public void Serialize(BuiltinType type, BinaryWriter writer)
        {
            void WriteByteBuffer(byte[] buf)
            {
                if (buf.LongLength > uint.MaxValue)
                {
                    throw new Exception("Serializing a buffer that is too large for SATS.");
                }

                writer.Write((uint)buf.LongLength);
                writer.Write(buf);
            }

            switch (type.type)
            {
                case BuiltinType.Type.Bool:
                    writer.Write(b);
                    break;
                case BuiltinType.Type.I8:
                    writer.Write(i8);
                    break;
                case BuiltinType.Type.U8:
                    writer.Write(u8);
                    break;
                case BuiltinType.Type.I16:
                    writer.Write(i16);
                    break;
                case BuiltinType.Type.U16:
                    writer.Write(u16);
                    break;
                case BuiltinType.Type.I32:
                    writer.Write(i32);
                    break;
                case BuiltinType.Type.U32:
                    writer.Write(u32);
                    break;
                case BuiltinType.Type.I64:
                    writer.Write(i64);
                    break;
                case BuiltinType.Type.U64:
                    writer.Write(u64);
                    break;
                case BuiltinType.Type.I128:
                    writer.Write(i128);
                    break;
                case BuiltinType.Type.U128:
                    writer.Write(u128);
                    break;
                case BuiltinType.Type.F32:
                    writer.Write(f32);
                    break;
                case BuiltinType.Type.F64:
                    writer.Write(f64);
                    break;
                case BuiltinType.Type.String:
                    WriteByteBuffer(System.Text.Encoding.UTF8.GetBytes(s));
                    break;
                case BuiltinType.Type.Array:
                    if (type.arrayType.type == AlgebraicType.Type.Builtin &&
                        type.arrayType.builtin.type == BuiltinType.Type.U8)
                    {
                        WriteByteBuffer(bytes);
                        break;
                    }

                    writer.Write(array.Count);
                    foreach (var entry in array)
                    {
                        entry.Serialize(type.arrayType, writer);
                    }
                    break;
                case BuiltinType.Type.Map:
                    break;
            }
        }

        public static BuiltinValue Deserialize(BuiltinType type, BinaryReader reader)
        {
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

            switch (type.type)
            {
                case BuiltinType.Type.Bool:
                    return Create(reader.ReadByte() != 0);
                case BuiltinType.Type.I8:
                    return Create(reader.ReadSByte());
                case BuiltinType.Type.U8:
                    return Create(reader.ReadByte());
                case BuiltinType.Type.I16:
                    return Create(reader.ReadInt16());
                case BuiltinType.Type.U16:
                    return Create(reader.ReadUInt16());
                case BuiltinType.Type.I32:
                    return Create(reader.ReadInt32());
                case BuiltinType.Type.U32:
                    return Create(reader.ReadUInt32());
                case BuiltinType.Type.I64:
                    return Create(reader.ReadInt64());
                case BuiltinType.Type.U64:
                    return Create(reader.ReadUInt64());
                case BuiltinType.Type.I128:
                    return CreateI128(reader.ReadBytes(16));
                case BuiltinType.Type.U128:
                    return CreateU128(reader.ReadBytes(16));
                case BuiltinType.Type.F32:
                    return Create(reader.ReadSingle());
                case BuiltinType.Type.F64:
                    return Create(reader.ReadDouble());
                case BuiltinType.Type.String:
                    return Create(System.Text.Encoding.UTF8.GetString(ReadByteArray()));
                case BuiltinType.Type.Array:
                    if (type.arrayType.type == AlgebraicType.Type.Builtin &&
                        type.arrayType.builtin.type == BuiltinType.Type.U8)
                    {
                        return CreateBytes(ReadByteArray());
                    }

                    var length = reader.ReadInt32();
                    var arrayResult = new List<AlgebraicValue>();
                    for (var x = 0; x < length; x++)
                    {
                        arrayResult.Add(AlgebraicValue.Deserialize(type.arrayType, reader));
                    }

                    return Create(arrayResult);
                case BuiltinType.Type.Map:
                {
                    var len = reader.ReadUInt32();
                    var mapResult = new Dictionary<AlgebraicValue, AlgebraicValue>();
                    for (var x = 0; x < len; x++)
                    {
                        var key = AlgebraicValue.Deserialize(type.mapType.keyType, reader);
                        var value = AlgebraicValue.Deserialize(type.mapType.valueType, reader);
                        mapResult.Add(key, value);
                    }

                    return Create(mapResult);
                }
                default:
                    throw new NotImplementedException();
            }
        }
    }

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
            var len = reader.ReadUInt32();
            var result = new ProductValue();
            for (var x = 0; x < len; x++)
            {
                result.elements.Add(AlgebraicValue.Deserialize(type.elements[x].algebraicType, reader));
            }

            return result;
        }
    }

    public class AlgebraicValue
    {
        public SumValue sum;
        public ProductValue product;
        public BuiltinValue builtin;

        public bool ToBool() => builtin.b;
        public sbyte ToI8() => builtin.i8;
        public byte ToU8() => builtin.u8;
        public short ToI16() => builtin.i16;
        public ushort ToU16() => builtin.u16;
        public int ToI32() => builtin.i32;
        public uint ToU32() => builtin.u32;
        public long ToI64() => builtin.i64;
        public ulong ToU64() => builtin.u64;
        public byte[] ToI128() => builtin.i128;
        public byte[] ToU128() => builtin.u128;
        public float ToF32() => builtin.f32;
        public double ToF64() => builtin.f64;
        public string ToFString() => builtin.s;
        public byte[] ToBytes() => builtin.bytes;
        public List<AlgebraicValue> ToArray() => builtin.array;
        public Dictionary<AlgebraicValue, AlgebraicValue> ToMap() => builtin.map;

        public static AlgebraicValue Bool(bool v) => new AlgebraicValue { builtin = new BuiltinValue { b = v } };
        public static AlgebraicValue I8(sbyte v) => new AlgebraicValue { builtin = new BuiltinValue { i8 = v } };
        public static AlgebraicValue U8(byte v) => new AlgebraicValue { builtin = new BuiltinValue { u8 = v } };
        public static AlgebraicValue I16(short v) => new AlgebraicValue { builtin = new BuiltinValue { i16 = v } };
        public static AlgebraicValue U16(ushort v) => new AlgebraicValue { builtin = new BuiltinValue { u16 = v } };
        public static AlgebraicValue I32(int v) => new AlgebraicValue { builtin = new BuiltinValue { i32 = v } };
        public static AlgebraicValue U32(uint v) => new AlgebraicValue { builtin = new BuiltinValue { u32 = v } };
        public static AlgebraicValue I64(long v) => new AlgebraicValue { builtin = new BuiltinValue { i64 = v } };
        public static AlgebraicValue U64(ulong v) => new AlgebraicValue { builtin = new BuiltinValue { u64 = v } };
        public static AlgebraicValue I128(byte[] v) => new AlgebraicValue { builtin = new BuiltinValue { i128 = v } };
        public static AlgebraicValue U128(byte[] v) => new AlgebraicValue { builtin = new BuiltinValue { u128 = v } };
        public static AlgebraicValue F32(float v) => new AlgebraicValue { builtin = new BuiltinValue { f32 = v } };
        public static AlgebraicValue F64(double v) => new AlgebraicValue { builtin = new BuiltinValue { f64 = v } };
        public static AlgebraicValue String(string v) => new AlgebraicValue { builtin = new BuiltinValue { s = v } };
        public static AlgebraicValue Bytes(byte[] v) => new AlgebraicValue { builtin = new BuiltinValue { bytes = v } };

        public bool GetBool() => builtin.b;
        public sbyte GetI8() => builtin.i8;
        public byte GetU8() => builtin.u8;
        public short GetI16() => builtin.i16;
        public ushort GetU16() => builtin.u16;
        public int GetI32() => builtin.i32;
        public uint GetU32() => builtin.u32;
        public long GetI64() => builtin.i64;
        public ulong GetU64() => builtin.u64;
        public byte[] GetI128() => builtin.i128;
        public byte[] GetU128() => builtin.u128;
        public float GetF32() => builtin.f32;
        public double GetF64() => builtin.f64;
        public string GetString() => builtin.s;
        public byte[] GetBytes() => builtin.bytes;
        public List<AlgebraicValue> GetArray() => builtin.array;
        public ProductValue GetProductValue() => product;
        public SumValue GetSumValue() => sum;

        public static AlgebraicValue Vec(AlgebraicType vecType, IEnumerable<AlgebraicValue> elements)
        {
            var array = new List<AlgebraicValue>();
            array.AddRange(elements);
            return new AlgebraicValue
            {
                builtin = new BuiltinValue
                {
                    array = array
                }
            };
        }

        public static AlgebraicValue Create(BuiltinValue value) => new AlgebraicValue { builtin = value };
        public static AlgebraicValue Create(ProductValue value) => new AlgebraicValue { product = value };
        public static AlgebraicValue Create(SumValue value) => new AlgebraicValue { sum = value };

        public static AlgebraicValue Deserialize(AlgebraicType type, BinaryReader reader)
        {
            switch (type.type)
            {
                case AlgebraicType.Type.Builtin:
                    return Create(BuiltinValue.Deserialize(type.builtin, reader));
                case AlgebraicType.Type.Product:
                    return Create(ProductValue.Deserialize(type.product, reader));
                case AlgebraicType.Type.Sum:
                    return Create(SumValue.Deserialize(type.sum, reader));
                default:
                    throw new NotImplementedException();
            }
        }

        public void Serialize(AlgebraicType type, BinaryWriter writer)
        {
            switch (type.type)
            {
                case AlgebraicType.Type.Builtin:
                    builtin.Serialize(type.builtin, writer);
                    break;
                case AlgebraicType.Type.Product:
                    product.Serialize(type.product, writer);
                    break;
                case AlgebraicType.Type.Sum:
                    sum.Serialize(type.sum, writer);
                    break;
                default:
                    throw new NotImplementedException();
            }
        }

        public static AlgebraicValue Array(List<AlgebraicValue> v) =>
            new AlgebraicValue { builtin = new BuiltinValue { array = v } };

        public static AlgebraicValue Map(Dictionary<AlgebraicValue, AlgebraicValue> v) =>
            new AlgebraicValue { builtin = new BuiltinValue { map = v } };
    }
}