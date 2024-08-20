using System.Diagnostics;
using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;

namespace SpacetimeDB;

public readonly partial struct Unit
{
    // Custom BSATN that returns an inline empty product type that can be recognised by SpacetimeDB.
    public readonly struct BSATN : IReadWrite<Unit>
    {
        public Unit Read(BinaryReader reader) => default;

        public void Write(BinaryWriter writer, Unit value) { }
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
    }

    // This must be explicitly forwarded to base, otherwise record will generate a new implementation.
    public override string ToString() => base.ToString();
}
