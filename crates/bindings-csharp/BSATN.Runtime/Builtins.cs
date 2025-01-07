namespace SpacetimeDB;

using System.Diagnostics;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;

internal static class Util
{
    public static Span<byte> AsBytes<T>(ref T val)
        where T : struct => MemoryMarshal.AsBytes(MemoryMarshal.CreateSpan(ref val, 1));

    /// <summary>
    /// Convert this object to a BIG-ENDIAN hex string.
    ///
    /// Big endian is almost always the correct convention here. It puts the most significant bytes
    /// of the number at the lowest indexes of the resulting string; assuming the string is printed
    /// with low indexes to the left, this will result in the correct hex number being displayed.
    ///
    /// (This might be wrong if the string is printed after, say, a unicode right-to-left marker.
    /// But, well, what can you do.)
    /// </summary>
    /// <typeparam name="T"></typeparam>
    /// <param name="val"></param>
    /// <returns></returns>
    public static string ToHexBigEndian<T>(T val)
        where T : struct
    {
        var bytes = AsBytes(ref val);
        // If host is little-endian, reverse the bytes.
        // Note that this reverses our stack copy of `val`, not the original value, and doesn't require heap `byte[]` allocation.
        if (BitConverter.IsLittleEndian)
        {
            bytes.Reverse();
        }
#if NET5_0_OR_GREATER
        return Convert.ToHexString(bytes);
#else
        /// Similar to `Convert.ToHexString`, but that method is not available in .NET Standard
        /// which we need to target for Unity support.
        return BitConverter.ToString(bytes.ToArray()).Replace("-", "");
#endif
    }

    /// <summary>
    /// Convert the passed byte array to a value of type T, optionally reversing it before performing the conversion.
    /// If the input is not reversed, it is treated as having the native endianness of the host system.
    /// (The endianness of the host system can be checked via System.BitConverter.IsLittleEndian.)
    /// </summary>
    /// <typeparam name="T"></typeparam>
    /// <param name="source"></param>
    /// <param name="littleEndian"></param>
    /// <returns></returns>
    public static T Read<T>(ReadOnlySpan<byte> source, bool littleEndian)
        where T : struct
    {
        Debug.Assert(
            source.Length == Marshal.SizeOf<T>(),
            $"Error while reading ${typeof(T).FullName}: expected source span to be {Marshal.SizeOf<T>()} bytes long, but was {source.Length} bytes."
        );

        var result = MemoryMarshal.Read<T>(source);

        if (littleEndian != BitConverter.IsLittleEndian)
        {
            AsBytes(ref result).Reverse();
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
#if NET5_0_OR_GREATER
        return Convert.FromHexString(hex);
#else
        // Manual implementation for .NET Standard compatibility.
        Debug.Assert(
            hex.Length % 2 == 0,
            $"Expected input string (\"{hex}\") to be of even length"
        );

        var NumberChars = hex.Length;
        var bytes = new byte[NumberChars / 2];
        for (var i = 0; i < NumberChars; i += 2)
        {
            bytes[i / 2] = Convert.ToByte(hex.Substring(i, 2), 16);
        }
        return bytes;
#endif
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
        where T : struct => Read<T>(StringToByteArray(hex), littleEndian: false);
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

[StructLayout(LayoutKind.Sequential)]
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
    public static Address? From(ReadOnlySpan<byte> bytes)
    {
        var addr = Util.Read<Address>(bytes, littleEndian: true);
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
    public static Address? FromBigEndian(ReadOnlySpan<byte> bytes)
    {
        var addr = Util.Read<Address>(bytes, littleEndian: false);
        return addr == default ? null : addr;
    }

    /// <summary>
    /// Create an Address from a hex string.
    /// </summary>
    /// <param name="hex"></param>
    /// <returns></returns>
    public static Address? FromHexString(string hex)
    {
        var addr = Util.ReadFromBigEndianHexString<Address>(hex);
        return addr == default ? null : addr;
    }

    public static Address Random()
    {
        var random = new Random();
        var addr = new Address();
        random.NextBytes(Util.AsBytes(ref addr));
        return addr;
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

[StructLayout(LayoutKind.Sequential)]
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
    public Identity(ReadOnlySpan<byte> bytes) => this = From(bytes);

    /// <summary>
    /// Create an Identity from a LITTLE-ENDIAN byte array.
    ///
    /// If you are parsing an Identity from a string, you probably want FromHexString instead,
    /// or, failing that, FromBigEndian.
    /// </summary>
    /// <param name="bytes"></param>
    public static Identity From(ReadOnlySpan<byte> bytes) =>
        Util.Read<Identity>(bytes, littleEndian: true);

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
    public static Identity FromBigEndian(ReadOnlySpan<byte> bytes) =>
        Util.Read<Identity>(bytes, littleEndian: false);

    /// <summary>
    /// Create an Identity from a hex string.
    /// </summary>
    /// <param name="hex"></param>
    /// <returns></returns>
    public static Identity FromHexString(string hex) =>
        Util.ReadFromBigEndianHexString<Identity>(hex);

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
    : SpacetimeDB.TaggedEnum<(TimeSpan Interval, DateTimeOffset Time)>
{
    // Manual expansion of what would be otherwise generated by the [SpacetimeDB.Type] codegen.
    public sealed record Interval(TimeSpan Interval_) : ScheduleAt;

    public sealed record Time(DateTimeOffset Time_) : ScheduleAt;

    public static implicit operator ScheduleAt(TimeSpan interval) => new Interval(interval);

    public static implicit operator ScheduleAt(DateTimeOffset time) => new Time(time);

    public readonly partial struct BSATN : IReadWrite<ScheduleAt>
    {
        [SpacetimeDB.Type]
        private partial record ScheduleAtRepr
            : SpacetimeDB.TaggedEnum<(TimeSpanRepr Interval, DateTimeOffsetRepr Time)>;

        private static readonly ScheduleAtRepr.BSATN ReprBSATN = new();

        public ScheduleAt Read(BinaryReader reader) =>
            ReprBSATN.Read(reader) switch
            {
                ScheduleAtRepr.Interval(var intervalRepr) => new Interval(intervalRepr.ToStd()),
                ScheduleAtRepr.Time(var timeRepr) => new Time(timeRepr.ToStd()),
                _ => throw new SwitchExpressionException(),
            };

        public void Write(BinaryWriter writer, ScheduleAt value)
        {
            ReprBSATN.Write(
                writer,
                value switch
                {
                    Interval(var interval) => new ScheduleAtRepr.Interval(new(interval)),
                    Time(var time) => new ScheduleAtRepr.Time(new(time)),
                    _ => throw new SwitchExpressionException(),
                }
            );
        }

        public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
            // Constructing a custom one instead of ScheduleAtRepr.GetAlgebraicType()
            // to avoid leaking the internal *Repr wrappers in generated SATS.
            new AlgebraicType.Sum(
                [
                    new("Interval", new AlgebraicType.U64(default)),
                    new("Time", new AlgebraicType.U64(default)),
                ]
            );
    }
}
