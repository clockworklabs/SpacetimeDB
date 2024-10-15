namespace SpacetimeDB;

using System.Diagnostics;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;

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

public record Address : BytesWrapper
{
    protected override int SIZE => 16;

    public Address() { }

    private Address(byte[] bytes)
        : base(bytes) { }

    public static Address? From(byte[] bytes)
    {
        if (bytes.All(b => b == 0))
        {
            return null;
        }
        return new(bytes);
    }

    public static Address Random()
    {
        var random = new Random();
        var addr = new Address();
        random.NextBytes(addr.bytes);
        return addr;
    }

    public readonly struct BSATN : IReadWrite<Address>
    {
        public Address Read(BinaryReader reader) => new(ReadRaw(reader));

        public void Write(BinaryWriter writer, Address value) => value.Write(writer);

        public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
            BytesWrapper.GetAlgebraicType(registrar, "__address_bytes");
    }

    // This must be explicitly forwarded to base, otherwise record will generate a new implementation.
    public override string ToString() => base.ToString();
}

public record Identity : BytesWrapper
{
    protected override int SIZE => 32;

    public Identity() { }

    public Identity(byte[] bytes)
        : base(bytes) { }

    public static Identity From(byte[] bytes) => new(bytes);

    public readonly struct BSATN : IReadWrite<Identity>
    {
        public Identity Read(BinaryReader reader) => new(ReadRaw(reader));

        public void Write(BinaryWriter writer, Identity value) => value.Write(writer);

        public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
            BytesWrapper.GetAlgebraicType(registrar, "__identity_bytes");
    }

    // This must be explicitly forwarded to base, otherwise record will generate a new implementation.
    public override string ToString() => base.ToString();
}

[StructLayout(LayoutKind.Sequential)] // we should be able to use it in FFI
[SpacetimeDB.Type] // we should be able to encode it to BSATN too
public readonly record struct Timestamp
{
    // This has a slightly wonky name, so just use the name directly.
    private long __timestamp_nanos_since_unix_epoch;

    public Timestamp(long NanosecondsSinceUnixEpoch)
    {
        this.__timestamp_nanos_since_unix_epoch = NanosecondsSinceUnixEpoch;
    }

    public static implicit operator DateTimeOffset(Timestamp t) => DateTimeOffset.UnixEpoch.AddMicroseconds(t.__timestamp_nanos_since_unix_epoch / 1000);
    public static implicit operator Timestamp(DateTimeOffset offset) => Timestamp(offset.Subtract(DateTimeOffset.UnixEpoch).Ticks * TimeSpan.NanosecondsPerTick);
}

[StructLayout(LayoutKind.Sequential)]
[SpacetimeDB.Type]
public readonly record struct TimeDuration
{
    private long __time_duration_nanoseconds;

    public TimeDuration(long Nanoseconds)
    {
        this.__time_duration_nanoseconds = Nanoseconds;
    }

    public static implicit operator TimeSpan(TimeDuration d) => TimeSpan(d.__time_duration_nanoseconds / TimeSpan.NanosecondsPerTick);
    public static implicit operator TimeDuration(TimeSpan timeSpan) => TimeDuration(timeSpan.Ticks * TimeSpan.NanosecondsPerTick);

}

[SpacetimeDB.Type]
public partial record ScheduleAt
    : SpacetimeDB.TaggedEnum<(Timestamp Time, TimeDuration Interval)>;
