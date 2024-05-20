using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;

namespace SpacetimeDB.SATS
{
    // [SpacetimeDB.Type] - we don't want this to be referenced via AlgebraicTypeRef as any other struct
    // because SpacetimeDB CLI `generate` command only recognises unit structs if they're inline.
    public partial struct Unit
    {
        private static readonly TypeInfo<Unit> satsTypeInfo =
            new(AlgebraicType.Unit, reader => default, (writer, value) => { });

        public static TypeInfo<Unit> GetSatsTypeInfo() => satsTypeInfo;
    }

    public class TypeInfo<T>(
        AlgebraicType algebraicType,
        Func<BinaryReader, T> read,
        Action<BinaryWriter, T> write
    )
    {
        public readonly AlgebraicType AlgebraicType = algebraicType;
        public readonly Func<BinaryReader, T> Read = read;
        public readonly Action<BinaryWriter, T> Write = write;

        public IEnumerable<T> ReadBytes(byte[] bytes)
        {
            using var stream = new MemoryStream(bytes);
            using var reader = new BinaryReader(stream);
            while (stream.Position < stream.Length)
            {
                yield return Read(reader);
            }
        }

        public byte[] ToBytes(T value)
        {
            using var stream = new MemoryStream();
            using var writer = new BinaryWriter(stream);
            Write(writer, value);
            return stream.ToArray();
        }

        public TypeInfo<object?> EraseType()
        {
            return new TypeInfo<object?>(
                AlgebraicType,
                reader => Read(reader),
                (writer, value) => Write(writer, (T)value!)
            );
        }
    }

    [SpacetimeDB.Type]
    public partial struct AggregateElement(string? name, AlgebraicType algebraicType)
    {
        public string? Name = name;
        public AlgebraicType AlgebraicType = algebraicType;
    }

    [SpacetimeDB.Type]
    public partial struct MapElement(AlgebraicType key, AlgebraicType value)
    {
        public AlgebraicType Key = key;
        public AlgebraicType Value = value;
    }

    [SpacetimeDB.Type]
    public partial record BuiltinType
        : SpacetimeDB.TaggedEnum<(
            Unit Bool,
            Unit I8,
            Unit U8,
            Unit I16,
            Unit U16,
            Unit I32,
            Unit U32,
            Unit I64,
            Unit U64,
            Unit I128,
            Unit U128,
            Unit F32,
            Unit F64,
            Unit String,
            AlgebraicType Array,
            MapElement Map
        )>
    {
        public static readonly TypeInfo<bool> BoolTypeInfo = new TypeInfo<bool>(
            new Bool(default),
            (reader) => reader.ReadBoolean(),
            (writer, value) => writer.Write(value)
        );

        public static readonly TypeInfo<sbyte> I8TypeInfo = new TypeInfo<sbyte>(
            new I8(default),
            (reader) => reader.ReadSByte(),
            (writer, value) => writer.Write(value)
        );

        public static readonly TypeInfo<byte> U8TypeInfo = new TypeInfo<byte>(
            new U8(default),
            (reader) => reader.ReadByte(),
            (writer, value) => writer.Write(value)
        );

        public static readonly TypeInfo<short> I16TypeInfo = new TypeInfo<short>(
            new I16(default),
            (reader) => reader.ReadInt16(),
            (writer, value) => writer.Write(value)
        );

        public static readonly TypeInfo<ushort> U16TypeInfo = new TypeInfo<ushort>(
            new U16(default),
            (reader) => reader.ReadUInt16(),
            (writer, value) => writer.Write(value)
        );

        public static readonly TypeInfo<int> I32TypeInfo = new TypeInfo<int>(
            new I32(default),
            (reader) => reader.ReadInt32(),
            (writer, value) => writer.Write(value)
        );

        public static readonly TypeInfo<uint> U32TypeInfo = new TypeInfo<uint>(
            new U32(default),
            (reader) => reader.ReadUInt32(),
            (writer, value) => writer.Write(value)
        );

        public static readonly TypeInfo<long> I64TypeInfo = new TypeInfo<long>(
            new I64(default),
            (reader) => reader.ReadInt64(),
            (writer, value) => writer.Write(value)
        );

        public static readonly TypeInfo<ulong> U64TypeInfo = new TypeInfo<ulong>(
            new U64(default),
            (reader) => reader.ReadUInt64(),
            (writer, value) => writer.Write(value)
        );

#if NET7_0_OR_GREATER
        public static readonly TypeInfo<Int128> I128TypeInfo = new TypeInfo<Int128>(
            new I128(default),
            (reader) => new Int128(reader.ReadUInt64(), reader.ReadUInt64()),
            (writer, value) =>
            {
                writer.Write((ulong)(value >> 64));
                writer.Write((ulong)value);
            }
        );

        public static readonly TypeInfo<UInt128> U128TypeInfo = new TypeInfo<UInt128>(
            new U128(default),
            (reader) => new UInt128(reader.ReadUInt64(), reader.ReadUInt64()),
            (writer, value) =>
            {
                writer.Write((ulong)(value >> 64));
                writer.Write((ulong)value);
            }
        );
#endif

        public static readonly TypeInfo<float> F32TypeInfo = new TypeInfo<float>(
            new F32(default),
            (reader) => reader.ReadSingle(),
            (writer, value) => writer.Write(value)
        );

        public static readonly TypeInfo<double> F64TypeInfo = new TypeInfo<double>(
            new F64(default),
            (reader) => reader.ReadDouble(),
            (writer, value) => writer.Write(value)
        );

        public static readonly TypeInfo<byte[]> BytesTypeInfo = new TypeInfo<byte[]>(
            new Array(U8TypeInfo.AlgebraicType),
            (reader) =>
            {
                var length = reader.ReadInt32();
                return reader.ReadBytes(length);
            },
            (writer, value) =>
            {
                writer.Write(value.Length);
                writer.Write(value);
            }
        );

        public static readonly TypeInfo<string> StringTypeInfo = new TypeInfo<string>(
            new String(default),
            (reader) => Encoding.UTF8.GetString(BytesTypeInfo.Read(reader)),
            (writer, value) => BytesTypeInfo.Write(writer, Encoding.UTF8.GetBytes(value))
        );

        private static IEnumerable<T> ReadEnumerable<T>(
            BinaryReader reader,
            Func<BinaryReader, T> readElement
        )
        {
            var length = reader.ReadInt32();
            return Enumerable.Range(0, length).Select((_) => readElement(reader));
        }

        private static void WriteEnumerable<T>(
            BinaryWriter writer,
            ICollection<T> enumerable,
            Action<BinaryWriter, T> writeElement
        )
        {
            writer.Write(enumerable.Count);
            foreach (var element in enumerable)
            {
                writeElement(writer, element);
            }
        }

        public static TypeInfo<A> MakeArrayLike<T, A>(
            Func<IEnumerable<T>, A> create,
            TypeInfo<T> elementTypeInfo
        )
            where A : ICollection<T>
        {
            return new TypeInfo<A>(
                new Array(elementTypeInfo.AlgebraicType),
                (reader) => create(ReadEnumerable(reader, elementTypeInfo.Read)),
                (writer, array) => WriteEnumerable(writer, array, elementTypeInfo.Write)
            );
        }

        public static TypeInfo<T[]> MakeArray<T>(TypeInfo<T> elementTypeInfo) =>
            MakeArrayLike(Enumerable.ToArray, elementTypeInfo);

        public static TypeInfo<List<T>> MakeList<T>(TypeInfo<T> elementTypeInfo) =>
            MakeArrayLike(Enumerable.ToList, elementTypeInfo);

        public static TypeInfo<Dictionary<K, V>> MakeMap<K, V>(TypeInfo<K> key, TypeInfo<V> value)
            where K : notnull
        {
            return new TypeInfo<Dictionary<K, V>>(
                new Map(new MapElement(key.AlgebraicType, value.AlgebraicType)),
                (reader) =>
                    ReadEnumerable(
                            reader,
                            (reader) => (Key: key.Read(reader), Value: value.Read(reader))
                        )
                        .ToDictionary((pair) => pair.Key, (pair) => pair.Value),
                (writer, map) =>
                    WriteEnumerable(
                        writer,
                        map,
                        (w, pair) =>
                        {
                            key.Write(w, pair.Key);
                            value.Write(w, pair.Value);
                        }
                    )
            );
        }

        private static Dictionary<Type, object> enumTypeInfoCache = [];

        public static TypeInfo<T> MakeEnum<T>()
            where T : struct, Enum, IConvertible
        {
            if (enumTypeInfoCache.TryGetValue(typeof(T), out var cached))
            {
                return (TypeInfo<T>)cached;
            }

            // plain enums are never recursive, so it should be fine to alloc & set typeref at once
            var typeRef = Module.FFI.AllocTypeRef();

            Module.FFI.SetTypeRef<T>(
                typeRef,
                new AlgebraicType.Sum(
                    Enum.GetNames(typeof(T))
                        .Select(name => new AggregateElement(name, AlgebraicType.Unit))
                        .ToArray()
                )
            );

            var typeInfo = new TypeInfo<T>(
                typeRef,
                (reader) => (T)Enum.ToObject(typeof(T), reader.ReadByte()),
                (writer, value) => writer.Write(Convert.ToByte(value))
            );

            enumTypeInfoCache[typeof(T)] = typeInfo;

            return typeInfo;
        }
    }

    [SpacetimeDB.Type]
    public partial record AlgebraicType
        : SpacetimeDB.TaggedEnum<(
            AggregateElement[] Sum,
            AggregateElement[] Product,
            BuiltinType Builtin,
            int Ref
        )>
    {
        public static implicit operator AlgebraicType(BuiltinType builtin) => new Builtin(builtin);

        public static readonly AlgebraicType Unit = new Product([]);

        public static readonly AlgebraicType Uninhabited = new Sum([]);

        // Special AlgebraicType that can be recognised by the SpacetimeDB `generate` CLI as an Option<T>.
        private static AlgebraicType MakeOptionAlgebraicType(AlgebraicType algebraicType) =>
            new Sum([new("some", algebraicType), new("none", Unit)]);

        public static TypeInfo<T?> MakeRefOption<T>(TypeInfo<T> typeInfo)
            where T : class
        {
            return new TypeInfo<T?>(
                MakeOptionAlgebraicType(typeInfo.AlgebraicType),
                (reader) => reader.ReadBoolean() ? null : typeInfo.Read(reader),
                (writer, value) =>
                {
                    writer.Write(value is null);
                    if (value is not null)
                    {
                        typeInfo.Write(writer, value);
                    }
                }
            );
        }

        // Yes, your eyes are not deceiving you... the body of this function is nearly identical
        // to MakeRefOption above. The only difference is the constraint on T.
        // Yes, this is dumb, but apparently you can't have *really* generic `T?` because,
        // despite identical bodies, compiler will desugar it very differently based on constraint.
        public static TypeInfo<T?> MakeValueOption<T>(TypeInfo<T> typeInfo)
            where T : struct
        {
            return new TypeInfo<T?>(
                MakeOptionAlgebraicType(typeInfo.AlgebraicType),
                (reader) => reader.ReadBoolean() ? null : typeInfo.Read(reader),
                (writer, value) =>
                {
                    writer.Write(!value.HasValue);
                    if (value.HasValue)
                    {
                        typeInfo.Write(writer, value.Value);
                    }
                }
            );
        }
    }
}
