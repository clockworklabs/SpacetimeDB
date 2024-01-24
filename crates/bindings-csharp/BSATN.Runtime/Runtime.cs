namespace SpacetimeDB.BSATN;

using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;

// Normally, T will be the same as the type the interface is implemented on.
// However, unlike Rust, C# doesn't allow implementing interfaces on built-in
// types like primitives, enums, nullable types, etc. So, we have to use
// wrapper classes sometimes that will take care of reading T while not being
// T themselves.
public interface IReadWrite<T>
{
    static abstract T Read(BinaryReader reader);
    static abstract void Write(BinaryWriter writer, T value);
}

public class Enum<T> : IReadWrite<T>
    where T : struct, System.Enum
{
    public static T Read(BinaryReader reader) =>
        (T)System.Enum.ToObject(typeof(T), reader.ReadByte());

    public static void Write(BinaryWriter writer, T value) =>
        writer.Write(System.Convert.ToByte(value));
}

public class RefOption<Inner, InnerRW> : IReadWrite<Inner?>
    where Inner : class
    where InnerRW : IReadWrite<Inner>
{
    public static Inner? Read(BinaryReader reader) =>
        reader.ReadBoolean() ? null : InnerRW.Read(reader);

    public static void Write(BinaryWriter writer, Inner? value)
    {
        writer.Write(value is null);
        if (value is not null)
        {
            InnerRW.Write(writer, value);
        }
    }
}

// This implementation is nearly identical to RefOption. The only difference is the constraint on T.
// Yes, this is dumb, but apparently you can't have *really* generic `T?` because,
// despite identical bodies, compiler will desugar it to very different
// types based on whether the constraint makes it a reference type or a value type.
public class ValueOption<Inner, InnerRW> : IReadWrite<Inner?>
    where Inner : struct
    where InnerRW : IReadWrite<Inner>
{
    public static Inner? Read(BinaryReader reader) =>
        reader.ReadBoolean() ? null : InnerRW.Read(reader);

    public static void Write(BinaryWriter writer, Inner? value)
    {
        writer.Write(!value.HasValue);
        if (value.HasValue)
        {
            InnerRW.Write(writer, value.Value);
        }
    }
}

public class Bool : IReadWrite<bool>
{
    public static bool Read(BinaryReader reader) => reader.ReadBoolean();

    public static void Write(BinaryWriter writer, bool value) => writer.Write(value);
}

public class U8 : IReadWrite<byte>
{
    public static byte Read(BinaryReader reader) => reader.ReadByte();

    public static void Write(BinaryWriter writer, byte value) => writer.Write(value);
}

public class U16 : IReadWrite<ushort>
{
    public static ushort Read(BinaryReader reader) => reader.ReadUInt16();

    public static void Write(BinaryWriter writer, ushort value) => writer.Write(value);
}

public class U32 : IReadWrite<uint>
{
    public static uint Read(BinaryReader reader) => reader.ReadUInt32();

    public static void Write(BinaryWriter writer, uint value) => writer.Write(value);
}

public class U64 : IReadWrite<ulong>
{
    public static ulong Read(BinaryReader reader) => reader.ReadUInt64();

    public static void Write(BinaryWriter writer, ulong value) => writer.Write(value);
}

#if NET7_0_OR_GREATER
public class U128 : IReadWrite<System.UInt128>
{
    public static System.UInt128 Read(BinaryReader reader) =>
        new(reader.ReadUInt64(), reader.ReadUInt64());

    public static void Write(BinaryWriter writer, System.UInt128 value)
    {
        writer.Write((ulong)(value >> 64));
        writer.Write((ulong)value);
    }
}
#endif

public class I8 : IReadWrite<sbyte>
{
    public static sbyte Read(BinaryReader reader) => reader.ReadSByte();

    public static void Write(BinaryWriter writer, sbyte value) => writer.Write(value);
}

public class I16 : IReadWrite<short>
{
    public static short Read(BinaryReader reader) => reader.ReadInt16();

    public static void Write(BinaryWriter writer, short value) => writer.Write(value);
}

public class I32 : IReadWrite<int>
{
    public static int Read(BinaryReader reader) => reader.ReadInt32();

    public static void Write(BinaryWriter writer, int value) => writer.Write(value);
}

public class I64 : IReadWrite<long>
{
    public static long Read(BinaryReader reader) => reader.ReadInt64();

    public static void Write(BinaryWriter writer, long value) => writer.Write(value);
}

#if NET7_0_OR_GREATER
public class I128 : IReadWrite<System.Int128>
{
    public static System.Int128 Read(BinaryReader reader) =>
        new(reader.ReadUInt64(), reader.ReadUInt64());

    public static void Write(BinaryWriter writer, System.Int128 value)
    {
        writer.Write((long)(value >> 64));
        writer.Write((long)value);
    }
}
#endif

public class F32 : IReadWrite<float>
{
    public static float Read(BinaryReader reader) => reader.ReadSingle();

    public static void Write(BinaryWriter writer, float value) => writer.Write(value);
}

public class F64 : IReadWrite<double>
{
    public static double Read(BinaryReader reader) => reader.ReadDouble();

    public static void Write(BinaryWriter writer, double value) => writer.Write(value);
}

class Enumerable<Element, ElementRW> : IReadWrite<IEnumerable<Element>>
    where ElementRW : IReadWrite<Element>
{
    public static IEnumerable<Element> Read(BinaryReader reader)
    {
        var count = reader.ReadInt32();
        for (var i = 0; i < count; i++)
        {
            yield return ElementRW.Read(reader);
        }
    }

    public static void Write(BinaryWriter writer, IEnumerable<Element> value)
    {
        writer.Write(value.Count());
        foreach (var element in value)
        {
            ElementRW.Write(writer, element);
        }
    }
}

public class Array<Element, ElementRW> : IReadWrite<Element[]>
    where ElementRW : IReadWrite<Element>
{
    public static Element[] Read(BinaryReader reader)
    {
        return Enumerable<Element, ElementRW>.Read(reader).ToArray();
    }

    public static void Write(BinaryWriter writer, Element[] value)
    {
        Enumerable<Element, ElementRW>.Write(writer, value);
    }
}

// Special case for byte arrays that can be dealt with more efficiently.
public class ByteArray : IReadWrite<byte[]>
{
    public static byte[] Read(BinaryReader reader)
    {
        return reader.ReadBytes(reader.ReadInt32());
    }

    public static void Write(BinaryWriter writer, byte[] value)
    {
        writer.Write(value.Length);
        writer.Write(value);
    }
}

// String is a special case of byte array with extra checks.
public class String : IReadWrite<string>
{
    public static string Read(BinaryReader reader) =>
        Encoding.UTF8.GetString(ByteArray.Read(reader));

    public static void Write(BinaryWriter writer, string value) =>
        ByteArray.Write(writer, Encoding.UTF8.GetBytes(value));
}

public class List<Element, ElementRW> : IReadWrite<List<Element>>
    where ElementRW : IReadWrite<Element>
{
    public static List<Element> Read(BinaryReader reader) =>
        Enumerable<Element, ElementRW>.Read(reader).ToList();

    public static void Write(BinaryWriter writer, List<Element> value) =>
        Enumerable<Element, ElementRW>.Write(writer, value);
}

class KeyValuePair<Key, Value, KeyRW, ValueRW> : IReadWrite<KeyValuePair<Key, Value>>
    where KeyRW : IReadWrite<Key>
    where ValueRW : IReadWrite<Value>
{
    public static KeyValuePair<Key, Value> Read(BinaryReader reader) =>
        new(KeyRW.Read(reader), ValueRW.Read(reader));

    public static void Write(BinaryWriter writer, KeyValuePair<Key, Value> value)
    {
        KeyRW.Write(writer, value.Key);
        ValueRW.Write(writer, value.Value);
    }
}

public class Dictionary<Key, Value, KeyRW, ValueRW> : IReadWrite<Dictionary<Key, Value>>
    where Key : notnull
    where KeyRW : IReadWrite<Key>
    where ValueRW : IReadWrite<Value>
{
    public static Dictionary<Key, Value> Read(BinaryReader reader) =>
        Enumerable<KeyValuePair<Key, Value>, KeyValuePair<Key, Value, KeyRW, ValueRW>>
            .Read(reader)
            .ToDictionary();

    public static void Write(BinaryWriter writer, Dictionary<Key, Value> value) =>
        Enumerable<KeyValuePair<Key, Value>, KeyValuePair<Key, Value, KeyRW, ValueRW>>.Write(
            writer,
            value
        );
}
