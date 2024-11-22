namespace SpacetimeDB;

using System.Diagnostics;
using System.Runtime.InteropServices;
using SpacetimeDB.BSATN;

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

    // Similarly, we need some constants that are not available in .NET Standard.
    public const long TicksPerMicrosecond = 10;
    public const long MicrosecondsPerSecond = 1_000_000;
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

// We manually implement the following few classes to customize their BSATN serialization.
// In particular, they are all "special" types, so they need to have their BSATN.GetAlgebraicType work in
// a special way. Rather than registering themselves in the Typespace, and returning an AlgebraicTypeRef,
// they return an AlgebraicType.Product directly, with a special property name that can be recognised by SpacetimeDB.
// This behaviour is ONLY used for these special types.
//
// If you need to update these types, remove the portion marked "// --- auto-generated ---",
// add a [SpacetimeDB.Type] annotation to the type, and enable "EmitCompilerGeneratedFiles" in BSATN.Runtime.csproj.
// Then, you can find the code that needs to be generated in `obj/`, and copy it here.
// Take extra care to update the code marked with "// --- customized ---".


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

    // --- auto-generated ---
    public readonly struct BSATN : IReadWrite<Address>
    {
        public Address Read(BinaryReader reader) =>
            new(new SpacetimeDB.BSATN.U128Stdb().Read(reader));

        public void Write(BinaryWriter writer, Address value) =>
            new SpacetimeDB.BSATN.U128Stdb().Write(writer, value.value);

        // --- / auto-generated ---

        // --- customized ---
        public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
            new AlgebraicType.Product([new("__address__", new AlgebraicType.U128(default))]);
        // --- / customized ---
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

        // --- / auto-generated ---

        // --- customized ---
        public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
            new AlgebraicType.Product([new("__identity__", new AlgebraicType.U256(default))]);
        // --- / customized ---
    }

    // This must be explicitly implemented, otherwise record will generate a new implementation.
    public override string ToString() => Util.ToHexBigEndian(value);
}

[StructLayout(LayoutKind.Sequential)] // we should be able to use it in FFI
public partial struct Timestamp(long microsecondsSinceUnixEpoch)
    : SpacetimeDB.BSATN.IStructuralReadWrite
{
    // This has a slightly wonky name, so just use the name directly.
    private long __timestamp_micros_since_unix_epoch__ = microsecondsSinceUnixEpoch;

    public readonly long MicrosecondsSinceUnixEpoch => __timestamp_micros_since_unix_epoch__;

    public static implicit operator DateTimeOffset(Timestamp t) =>
        DateTimeOffset.UnixEpoch.AddTicks(
            t.__timestamp_micros_since_unix_epoch__ * Util.TicksPerMicrosecond
        );

    public static implicit operator Timestamp(DateTimeOffset offset) =>
        new Timestamp(offset.Subtract(DateTimeOffset.UnixEpoch).Ticks / Util.TicksPerMicrosecond);

    // For backwards-compatibility.
    public readonly DateTimeOffset ToStd() => this;

    // Should be consistent with Rust implementation of Display.
    public override string ToString()
    {
        var sign = MicrosecondsSinceUnixEpoch < 0 ? "-" : "";
        var pos = Math.Abs(MicrosecondsSinceUnixEpoch);
        var secs = pos / Util.MicrosecondsPerSecond;
        var microsRemaining = pos % Util.MicrosecondsPerSecond;
        return $"{sign}{secs}.{microsRemaining:D6}";
    }

    // --- auto-generated ---
    public void ReadFields(System.IO.BinaryReader reader)
    {
        __timestamp_micros_since_unix_epoch__ = BSATN.__timestamp_micros_since_unix_epoch__.Read(
            reader
        );
    }

    public void WriteFields(System.IO.BinaryWriter writer)
    {
        BSATN.__timestamp_micros_since_unix_epoch__.Write(
            writer,
            __timestamp_micros_since_unix_epoch__
        );
    }

    public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<SpacetimeDB.Timestamp>
    {
        internal static readonly SpacetimeDB.BSATN.I64 __timestamp_micros_since_unix_epoch__ =
            new();

        public SpacetimeDB.Timestamp Read(System.IO.BinaryReader reader) =>
            SpacetimeDB.BSATN.IStructuralReadWrite.Read<SpacetimeDB.Timestamp>(reader);

        public void Write(System.IO.BinaryWriter writer, SpacetimeDB.Timestamp value)
        {
            value.WriteFields(writer);
        }

        // --- / auto-generated ---

        // --- customized ---
        public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            new AlgebraicType.Product(
                [new("__timestamp_micros_since_unix_epoch__", new AlgebraicType.I64(default))]
            );
        // --- / customized ---
    }
}

[StructLayout(LayoutKind.Sequential)]
public partial struct TimeDuration(long microseconds) : SpacetimeDB.BSATN.IStructuralReadWrite
{
    private long __time_duration_micros__ = microseconds;

    public readonly long Microseconds => __time_duration_micros__;

    public static implicit operator TimeSpan(TimeDuration d) =>
        new TimeSpan(d.__time_duration_micros__ * Util.TicksPerMicrosecond);

    public static implicit operator TimeDuration(TimeSpan timeSpan) =>
        new TimeDuration(timeSpan.Ticks / Util.TicksPerMicrosecond);

    // For backwards-compatibility.
    public readonly TimeSpan ToStd() => this;

    // Should be consistent with Rust implementation of Display.
    public override string ToString()
    {
        var sign = Microseconds < 0 ? "-" : "+";
        var pos = Math.Abs(Microseconds);
        var secs = pos / Util.MicrosecondsPerSecond;
        var microsRemaining = pos % Util.MicrosecondsPerSecond;
        return $"{sign}{secs}.{microsRemaining:D6}";
    }

    // --- auto-generated ---

    public void ReadFields(System.IO.BinaryReader reader)
    {
        __time_duration_micros__ = BSATN.__time_duration_micros__.Read(reader);
    }

    public void WriteFields(System.IO.BinaryWriter writer)
    {
        BSATN.__time_duration_micros__.Write(writer, __time_duration_micros__);
    }

    public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<SpacetimeDB.TimeDuration>
    {
        internal static readonly SpacetimeDB.BSATN.I64 __time_duration_micros__ = new();

        public SpacetimeDB.TimeDuration Read(System.IO.BinaryReader reader) =>
            SpacetimeDB.BSATN.IStructuralReadWrite.Read<SpacetimeDB.TimeDuration>(reader);

        public void Write(System.IO.BinaryWriter writer, SpacetimeDB.TimeDuration value)
        {
            value.WriteFields(writer);
        }

        // --- / auto-generated ---

        // --- customized ---
        public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            new AlgebraicType.Product(
                [new("__time_duration_micros__", new AlgebraicType.I64(default))]
            );

        // --- / customized ---
    }
}

public partial record ScheduleAt : SpacetimeDB.TaggedEnum<(TimeDuration Interval, Timestamp Time)>
{
    public static implicit operator ScheduleAt(TimeDuration duration) => new Interval(duration);

    public static implicit operator ScheduleAt(Timestamp time) => new Time(time);

    public static implicit operator ScheduleAt(TimeSpan duration) => new Interval(duration);

    public static implicit operator ScheduleAt(DateTimeOffset time) => new Time(time);

    // --- auto-generated ---
    private ScheduleAt() { }

    internal enum @enum : byte
    {
        Interval,
        Time,
    }

    public sealed record Interval(SpacetimeDB.TimeDuration Interval_) : ScheduleAt;

    public sealed record Time(SpacetimeDB.Timestamp Time_) : ScheduleAt;

    public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<SpacetimeDB.ScheduleAt>
    {
        internal static readonly SpacetimeDB.BSATN.Enum<@enum> __enumTag = new();
        internal static readonly SpacetimeDB.TimeDuration.BSATN Interval = new();
        internal static readonly SpacetimeDB.Timestamp.BSATN Time = new();

        public SpacetimeDB.ScheduleAt Read(System.IO.BinaryReader reader) =>
            __enumTag.Read(reader) switch
            {
                @enum.Interval => new Interval(Interval.Read(reader)),
                @enum.Time => new Time(Time.Read(reader)),
                _ => throw new System.InvalidOperationException(
                    "Invalid tag value, this state should be unreachable."
                ),
            };

        public void Write(System.IO.BinaryWriter writer, SpacetimeDB.ScheduleAt value)
        {
            switch (value)
            {
                case Interval(var inner):
                    __enumTag.Write(writer, @enum.Interval);
                    Interval.Write(writer, inner);
                    break;

                case Time(var inner):
                    __enumTag.Write(writer, @enum.Time);
                    Time.Write(writer, inner);
                    break;
            }
        }

        // --- / auto-generated ---

        // --- customized ---
        public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(
            SpacetimeDB.BSATN.ITypeRegistrar registrar
        ) =>
            registrar.RegisterType<SpacetimeDB.ScheduleAt>(
                _ => new SpacetimeDB.BSATN.AlgebraicType.Sum(
                    new SpacetimeDB.BSATN.AggregateElement[]
                    {
                        new(nameof(Interval), Interval.GetAlgebraicType(registrar)),
                        new(nameof(Time), Time.GetAlgebraicType(registrar)),
                    }
                )
            );
        // --- / customized ---
    }
}
