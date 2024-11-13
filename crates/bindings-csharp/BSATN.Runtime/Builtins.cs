namespace SpacetimeDB;

using System.Diagnostics;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;

internal static class Util
{
    /// <summary>
    /// Convert this object to a BIG-ENDIAN hex string.
    ///
    /// Big endian is almost always the correct convention here. It puts the most significant bytes
    /// of the number at the lowest indexes of the resulting string; assuming the string is printed
    /// with low indexes to the left, this will result in the correct hex number being displayed.
    ///
    /// (This might be wrong if the string is printed after, say, a unicode right-to-left marker.
    /// But, well, what can you do.)
    ///
    /// Similar to `Convert.ToHexString`, but that method is not available in .NET Standard
    /// which we need to target for Unity support.
    /// </summary>
    /// <typeparam name="T"></typeparam>
    /// <param name="val"></param>
    /// <returns></returns>
    public static string ToHexBigEndian<T>(T val)
        where T : struct => BitConverter.ToString(AsBytesBigEndian(val).ToArray()).Replace("-", "");

    /// <summary>
    /// Read a value of type T from the passed span, which is assumed to be in little-endian format.
    /// The behavior of this method is independent of the endianness of the host, unlike MemoryMarshal.Read.
    /// </summary>
    /// <typeparam name="T"></typeparam>
    /// <param name="source"></param>
    /// <returns></returns>
    public static T ReadLittleEndian<T>(ReadOnlySpan<byte> source)
        where T : struct => Read<T>(source, !BitConverter.IsLittleEndian);

    /// <summary>
    /// Read a value of type T from the passed span, which is assumed to be in big-endian format.
    /// The behavior of this method is independent of the endianness of the host, unlike MemoryMarshal.Read.
    /// </summary>
    /// <typeparam name="T"></typeparam>
    /// <param name="source"></param>
    /// <returns></returns>
    public static T ReadBigEndian<T>(ReadOnlySpan<byte> source)
        where T : struct => Read<T>(source, BitConverter.IsLittleEndian);

    /// <summary>
    /// Convert the passed byte array to a value of type T, optionally reversing it before performing the conversion.
    /// If the input is not reversed, it is treated as having the native endianness of the host system.
    /// (The endianness of the host system can be checked via System.BitConverter.IsLittleEndian.)
    /// </summary>
    /// <typeparam name="T"></typeparam>
    /// <param name="source"></param>
    /// <param name="reverse"></param>
    /// <returns></returns>
    static T Read<T>(ReadOnlySpan<byte> source, bool reverse)
        where T : struct
    {
        Debug.Assert(
            source.Length == Marshal.SizeOf<T>(),
            $"Error while reading ${typeof(T).FullName}: expected source span to be {Marshal.SizeOf<T>()} bytes long, but was {source.Length} bytes."
        );

        var result = MemoryMarshal.Read<T>(source);

        if (reverse)
        {
            var resultSpan = MemoryMarshal.CreateSpan(ref result, 1);
            MemoryMarshal.AsBytes(resultSpan).Reverse();
        }

        return result;
    }

    /// <summary>
    /// Convert the passed T to a little-endian byte array.
    /// The behavior of this method is independent of the endianness of the host, unlike MemoryMarshal.Read.
    /// </summary>
    /// <typeparam name="T"></typeparam>
    /// <param name="source"></param>
    /// <returns></returns>
    public static byte[] AsBytesLittleEndian<T>(T source)
        where T : struct => AsBytes(source, !BitConverter.IsLittleEndian);

    /// <summary>
    /// Convert the passed T to a big-endian byte array.
    /// The behavior of this method is independent of the endianness of the host, unlike MemoryMarshal.Read.
    /// </summary>
    /// <typeparam name="T"></typeparam>
    /// <param name="source"></param>
    /// <returns></returns>
    public static byte[] AsBytesBigEndian<T>(T source)
        where T : struct => AsBytes<T>(source, BitConverter.IsLittleEndian);

    /// <summary>
    /// Convert the passed T to a byte array, and optionally reverse the array before returning it.
    /// If the output is not reversed, it will have the native endianness of the host system.
    /// (The endianness of the host system can be checked via System.BitConverter.IsLittleEndian.)
    /// </summary>
    /// <typeparam name="T"></typeparam>
    /// <param name="source"></param>
    /// <param name="reverse"></param>
    /// <returns></returns>
    static byte[] AsBytes<T>(T source, bool reverse)
        where T : struct
    {
        var result = MemoryMarshal.AsBytes([source]).ToArray();
        if (reverse)
        {
            Array.Reverse(result, 0, result.Length);
        }
        return result;
    }

    /// <summary>
    /// Convert a hex string to a byte array.
    /// </summary>
    /// <param name="hex"></param>
    /// <returns></returns>
    public static byte[] StringToByteArray(string hex)
    {
        Debug.Assert(
            hex.Length % 2 == 0,
            $"Expected input string (\"{hex}\") to be of even length"
        );

        foreach (var c in hex)
        {
            Debug.Assert(
                (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F'),
                $"Expected input string (\"{hex}\") to be hexadecimal"
            );
        }

        var NumberChars = hex.Length;
        var bytes = new byte[NumberChars / 2];
        for (var i = 0; i < NumberChars; i += 2)
        {
            bytes[i / 2] = Convert.ToByte(hex.Substring(i, 2), 16);
        }
        return bytes;
    }

    /// <summary>
    /// Read a value from a "big-endian" hex string.
    /// All hex strings we expect to encounter are big-endian (store most significant bytes
    /// at low indexes) so this should always be used.
    /// </summary>
    /// <typeparam name="T"></typeparam>
    /// <param name="hex"></param>
    /// <returns></returns>
    public static T ReadFromBigEndianHexString<T>(string hex)
        where T : struct => ReadBigEndian<T>(StringToByteArray(hex));
}

public readonly partial struct Unit
{
    // Custom BSATN that returns an inline empty product type that can be recognised by SpacetimeDB.
    public readonly struct BSATN : IReadWrite<Unit>
    {
        public Unit Read(BinaryReader reader) => default;

        public void Write(BinaryWriter writer, Unit value) { }

        public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
            new AlgebraicType.Product([]);
    }
}

public readonly record struct Address
{
    private readonly U128 value;

    internal Address(U128 v) => value = v;

    /// <summary>
    /// Create an Address from a LITTLE-ENDIAN byte array.
    ///
    /// If you are parsing an Address from a string, you probably want FromHexString instead,
    /// or, failing that, FromBigEndian.
    ///
    /// Returns null if the resulting address is the default.
    /// </summary>
    /// <param name="bytes"></param>
    public static Address? From(byte[] bytes)
    {
        var addr = new Address(Util.ReadLittleEndian<U128>(bytes));
        return addr == default ? null : addr;
    }

    /// <summary>
    /// Create an Address from a BIG-ENDIAN byte array.
    ///
    /// This method is the correct choice if you have converted the bytes of a hexadecimal-formatted Address
    /// to a byte array in the following way:
    ///
    /// "0xb0b1b2..."
    /// ->
    /// [0xb0, 0xb1, 0xb2, ...]
    ///
    /// Returns null if the resulting address is the default.
    /// </summary>
    /// <param name="bytes"></param>
    public static Address? FromBigEndian(byte[] bytes)
    {
        var addr = new Address(Util.ReadBigEndian<U128>(bytes));
        return addr == default ? null : addr;
    }

    /// <summary>
    /// Create an Address from a hex string.
    /// </summary>
    /// <param name="hex"></param>
    /// <returns></returns>
    public static Address? FromHexString(string hex)
    {
        var addr = new Address(Util.ReadFromBigEndianHexString<U128>(hex));
        return addr == default ? null : addr;
    }

    public static Address Random()
    {
        var random = new Random();
        var bytes = new byte[16];
        random.NextBytes(bytes);
        return Address.From(bytes) ?? default;
    }

    public readonly struct BSATN : IReadWrite<Address>
    {
        public Address Read(BinaryReader reader) =>
            new(new SpacetimeDB.BSATN.U128Stdb().Read(reader));

        public void Write(BinaryWriter writer, Address value) =>
            new SpacetimeDB.BSATN.U128Stdb().Write(writer, value.value);

        public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
            new AlgebraicType.Product([new("__address__", new AlgebraicType.U128(default))]);
    }

    public override string ToString() => Util.ToHexBigEndian(value);
}

public readonly record struct Identity
{
    private readonly U256 value;

    internal Identity(U256 val) => value = val;

    /// <summary>
    /// Create an Identity from a LITTLE-ENDIAN byte array.
    ///
    /// If you are parsing an Identity from a string, you probably want FromHexString instead,
    /// or, failing that, FromBigEndian.
    /// </summary>
    /// <param name="bytes"></param>
    public Identity(byte[] bytes)
    {
        value = Util.ReadLittleEndian<U256>(bytes);
    }

    /// <summary>
    /// Create an Identity from a LITTLE-ENDIAN byte array.
    ///
    /// If you are parsing an Identity from a string, you probably want FromHexString instead,
    /// or, failing that, FromBigEndian.
    /// </summary>
    /// <param name="bytes"></param>
    public static Identity From(byte[] bytes) => new(bytes);

    /// <summary>
    /// Create an Identity from a BIG-ENDIAN byte array.
    ///
    /// This method is the correct choice if you have converted the bytes of a hexadecimal-formatted `Identity`
    /// to a byte array in the following way:
    ///
    /// "0xb0b1b2..."
    /// ->
    /// [0xb0, 0xb1, 0xb2, ...]
    /// </summary>
    /// <param name="bytes"></param>
    public static Identity FromBigEndian(byte[] bytes)
    {
        return new Identity(Util.ReadBigEndian<U256>(bytes));
    }

    /// <summary>
    /// Create an Identity from a hex string.
    /// </summary>
    /// <param name="hex"></param>
    /// <returns></returns>
    public static Identity FromHexString(string hex) =>
        new Identity(Util.ReadFromBigEndianHexString<U256>(hex));

    public readonly struct BSATN : IReadWrite<Identity>
    {
        public Identity Read(BinaryReader reader) => new(new SpacetimeDB.BSATN.U256().Read(reader));

        public void Write(BinaryWriter writer, Identity value) =>
            new SpacetimeDB.BSATN.U256().Write(writer, value.value);

        public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
            new AlgebraicType.Product([new("__identity__", new AlgebraicType.U256(default))]);
    }

    // This must be explicitly forwarded to base, otherwise record will generate a new implementation.
    public override string ToString() => Util.ToHexBigEndian(value);
}

// [SpacetimeDB.Type] - we have custom representation of time in microseconds, so implementing BSATN manually
public abstract partial record ScheduleAt
    : SpacetimeDB.TaggedEnum<(DateTimeOffset Time, TimeSpan Interval)>
{
    // Manual expansion of what would be otherwise generated by the [SpacetimeDB.Type] codegen.
    public sealed record Time(DateTimeOffset Time_) : ScheduleAt;

    public sealed record Interval(TimeSpan Interval_) : ScheduleAt;

    public static implicit operator ScheduleAt(DateTimeOffset time) => new Time(time);

    public static implicit operator ScheduleAt(TimeSpan interval) => new Interval(interval);

    public readonly partial struct BSATN : IReadWrite<ScheduleAt>
    {
        [SpacetimeDB.Type]
        private partial record ScheduleAtRepr
            : SpacetimeDB.TaggedEnum<(DateTimeOffsetRepr Time, TimeSpanRepr Interval)>;

        private static readonly ScheduleAtRepr.BSATN ReprBSATN = new();

        public ScheduleAt Read(BinaryReader reader) =>
            ReprBSATN.Read(reader) switch
            {
                ScheduleAtRepr.Time(var timeRepr) => new Time(timeRepr.ToStd()),
                ScheduleAtRepr.Interval(var intervalRepr) => new Interval(intervalRepr.ToStd()),
                _ => throw new SwitchExpressionException(),
            };

        public void Write(BinaryWriter writer, ScheduleAt value)
        {
            ReprBSATN.Write(
                writer,
                value switch
                {
                    Time(var time) => new ScheduleAtRepr.Time(new(time)),
                    Interval(var interval) => new ScheduleAtRepr.Interval(new(interval)),
                    _ => throw new SwitchExpressionException(),
                }
            );
        }

        public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
            // Constructing a custom one instead of ScheduleAtRepr.GetAlgebraicType()
            // to avoid leaking the internal *Repr wrappers in generated SATS.
            new AlgebraicType.Sum(
                [
                    new("Time", new AlgebraicType.U64(default)),
                    new("Interval", new AlgebraicType.U64(default)),
                ]
            );
    }
}
