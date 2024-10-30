namespace SpacetimeDB;

using System.Diagnostics;
using System.Runtime.InteropServices;
using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;

internal static class Util
{
    // Same as `Convert.ToHexString`, but that method is not available in .NET Standard
    // which we need to target for Unity support.
    public static string ToHex<T>(T val)
        where T : struct =>
        BitConverter.ToString(MemoryMarshal.AsBytes([val]).ToArray()).Replace("-", "");

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

// A helper for special wrappers around byte arrays like Identity and Address.
// Makes them equatable, stringifiable, checks length, etc.
public abstract record BytesWrapper
{
    protected abstract int SIZE { get; }

    protected readonly byte[] bytes;

    protected BytesWrapper()
    {
        bytes = new byte[SIZE];
    }

    // We can't hide the base class itself, but at least we can hide the constructor.
    protected BytesWrapper(byte[] bytes)
    {
        Debug.Assert(bytes.Length == SIZE);
        this.bytes = bytes;
    }

    public virtual bool Equals(BytesWrapper? other) =>
        ByteArrayComparer.Instance.Equals(bytes, other?.bytes);

    public override int GetHashCode() => ByteArrayComparer.Instance.GetHashCode(bytes);

    // Same as `Convert.ToHexString`, but that method is not available in .NET Standard
    // which we need to target for Unity support.
    public override string ToString() => BitConverter.ToString(bytes).Replace("-", "");

    protected static byte[] ReadRaw(BinaryReader reader) => ByteArray.Instance.Read(reader);

    protected void Write(BinaryWriter writer) => ByteArray.Instance.Write(writer, bytes);

    // Custom BSATN that returns an inline product type with special property name that can be recognised by SpacetimeDB.
    protected static AlgebraicType GetAlgebraicType(
        ITypeRegistrar registrar,
        string wrapperPropertyName
    ) =>
        new AlgebraicType.Product(
            [new(wrapperPropertyName, ByteArray.Instance.GetAlgebraicType(registrar))]
        );
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

    public static Address? From(byte[] bytes)
    {
        Debug.Assert(bytes.Length == 16);
        var addr = new Address(MemoryMarshal.Read<U128>(bytes));
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

    public override string ToString() => Util.ToHex(value);
}

public readonly record struct Identity
{
    private readonly U256 value;

    internal Identity(U256 val) => value = val;

    public Identity(byte[] bytes)
    {
        Debug.Assert(bytes.Length == 32);
        value = MemoryMarshal.Read<U256>(bytes);
    }

    public static Identity From(byte[] bytes) => new(bytes);

    // --- auto-generated ---
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

    // This must be explicitly forwarded to base, otherwise record will generate a new implementation.
    public override string ToString() => Util.ToHex(value);
}

[StructLayout(LayoutKind.Sequential)] // we should be able to use it in FFI
public partial struct Timestamp(long microsecondsSinceUnixEpoch) : SpacetimeDB.BSATN.IStructuralReadWrite

{
    // This has a slightly wonky name, so just use the name directly.
    private long __timestamp_micros_since_unix_epoch__ = microsecondsSinceUnixEpoch;

    public readonly long MicrosecondsSinceUnixEpoch => __timestamp_micros_since_unix_epoch__;

    public static implicit operator DateTimeOffset(Timestamp t) => DateTimeOffset.UnixEpoch.AddTicks(t.__timestamp_micros_since_unix_epoch__ * Util.TicksPerMicrosecond);
    public static implicit operator Timestamp(DateTimeOffset offset) => new Timestamp(offset.Subtract(DateTimeOffset.UnixEpoch).Ticks / Util.TicksPerMicrosecond);


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
        __timestamp_micros_since_unix_epoch__ = BSATN.__timestamp_micros_since_unix_epoch__.Read(reader);
    }

    public void WriteFields(System.IO.BinaryWriter writer)
    {
        BSATN.__timestamp_micros_since_unix_epoch__.Write(writer, __timestamp_micros_since_unix_epoch__);
    }

    public readonly partial struct BSATN : SpacetimeDB.BSATN.IReadWrite<SpacetimeDB.Timestamp>
    {
        internal static readonly SpacetimeDB.BSATN.I64 __timestamp_micros_since_unix_epoch__ = new();

        public SpacetimeDB.Timestamp Read(System.IO.BinaryReader reader) => SpacetimeDB.BSATN.IStructuralReadWrite.Read<SpacetimeDB.Timestamp>(reader);

        public void Write(System.IO.BinaryWriter writer, SpacetimeDB.Timestamp value)
        {
            value.WriteFields(writer);
        }
        // --- / auto-generated ---

        // --- customized ---
        public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(SpacetimeDB.BSATN.ITypeRegistrar registrar) =>
            new AlgebraicType.Product([new("__timestamp_micros_since_unix_epoch__", new AlgebraicType.I64(default))]);
        // --- / customized ---
    }
}

[StructLayout(LayoutKind.Sequential)]
public partial struct TimeDuration(long microseconds) : SpacetimeDB.BSATN.IStructuralReadWrite
{
    private long __time_duration_micros__ = microseconds;

    public readonly long Microseconds => __time_duration_micros__;

    public static implicit operator TimeSpan(TimeDuration d) => new TimeSpan(d.__time_duration_micros__ / Util.MicrosecondsPerTick);
    public static implicit operator TimeDuration(TimeSpan timeSpan) => new TimeDuration(timeSpan.Ticks * Util.MicrosecondsPerTick);

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

        public SpacetimeDB.TimeDuration Read(System.IO.BinaryReader reader) => SpacetimeDB.BSATN.IStructuralReadWrite.Read<SpacetimeDB.TimeDuration>(reader);

        public void Write(System.IO.BinaryWriter writer, SpacetimeDB.TimeDuration value)
        {
            value.WriteFields(writer);
        }

        // --- / auto-generated ---

        // --- customized ---
        public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(SpacetimeDB.BSATN.ITypeRegistrar registrar) =>
            new AlgebraicType.Product([new("__time_duration_micros__", new AlgebraicType.I64(default))]);

        // --- / customized ---
    }
}

public partial record ScheduleAt
    : SpacetimeDB.TaggedEnum<(TimeDuration Interval, Timestamp Time)>
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

        public SpacetimeDB.ScheduleAt Read(System.IO.BinaryReader reader) => __enumTag.Read(reader) switch
        {
            @enum.Interval => new Interval(Interval.Read(reader)),
            @enum.Time => new Time(Time.Read(reader)),
            _ => throw new System.InvalidOperationException("Invalid tag value, this state should be unreachable.")
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
        public SpacetimeDB.BSATN.AlgebraicType GetAlgebraicType(SpacetimeDB.BSATN.ITypeRegistrar registrar) =>
            registrar.RegisterType<SpacetimeDB.ScheduleAt>(_ => new SpacetimeDB.BSATN.AlgebraicType.Sum(new SpacetimeDB.BSATN.AggregateElement[] {
                new(nameof(Interval), Interval.GetAlgebraicType(registrar)),
                new(nameof(Time), Time.GetAlgebraicType(registrar)),
            }));
        // --- / customized ---
    }
}
