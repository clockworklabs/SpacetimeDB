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

// We store time information in microseconds in internal usages.
//
// These utils allow to encode it as such in FFI and BSATN contexts
// and convert to standard C# types.

[StructLayout(LayoutKind.Sequential)] // we should be able to use it in FFI
[SpacetimeDB.Type] // we should be able to encode it to BSATN too
public partial struct DateTimeOffsetRepr(DateTimeOffset time)
{
    public ulong MicrosecondsSinceEpoch = (ulong)time.Ticks / 10;

    public readonly DateTimeOffset ToStd() =>
        DateTimeOffset.UnixEpoch.AddTicks(10 * (long)MicrosecondsSinceEpoch);
}

[StructLayout(LayoutKind.Sequential)] // we should be able to use it in FFI
[SpacetimeDB.Type] // we should be able to encode it to BSATN too
public partial struct TimeSpanRepr(TimeSpan duration)
{
    public ulong Microseconds = (ulong)duration.Ticks / 10;

    public readonly TimeSpan ToStd() => TimeSpan.FromTicks(10 * (long)Microseconds);
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
