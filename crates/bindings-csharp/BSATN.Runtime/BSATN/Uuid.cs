namespace SpacetimeDB;

using System.Buffers.Binary;
using System.Runtime.InteropServices;
using SpacetimeDB.BSATN;

/// <summary>
/// A universally unique identifier (UUID).
///
/// Generate `NIL`, random (`v4`), and time-ordered (`v7`) UUIDs.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
public readonly record struct Uuid : IEquatable<Uuid>, IComparable, IComparable<Uuid>
{
    private readonly U128 value;

    public Uuid(U128 val) => value = val;

    /// <summary>
    /// The nil UUID (all bits set to zero).
    /// </summary>
    public static readonly Uuid NIL = new(new U128());

    /// <summary>
    /// The max UUID (all bits set to one).
    /// </summary>
    public static readonly Uuid MAX = new(new U128(ulong.MaxValue, ulong.MaxValue));

    /// <summary>
    /// Create a UUIDv4 from explicit random bytes.
    /// </summary>
    /// <remarks>
    /// This method assumes the provided bytes are already sufficiently random;
    /// it will only set the appropriate bits for the UUID version and variant.
    /// </remarks>
    /// <example>
    /// <code>
    /// var randomBytes = new byte[16];
    /// RandomNumberGenerator.Fill(randomBytes);
    /// var uuid = Uuid.FromRandomBytesV4(randomBytes);
    /// Console.WriteLine(uuid);
    /// // Output: 166a036f-90f3-714a-9731-c8814b70e326
    /// </code>
    /// </example>
    public static Uuid FromRandomBytesV4(ReadOnlySpan<byte> randomBytes)
    {
        if (randomBytes.Length != 16)
        {
            throw new ArgumentException("Must be 16 bytes", nameof(randomBytes));
        }

        Span<byte> bytes = stackalloc byte[16];
        randomBytes.CopyTo(bytes);
        bytes[6] = (byte)((bytes[6] & 0x0F) | 0x40); // version 4
        bytes[8] = (byte)((bytes[8] & 0x3F) | 0x80); // variant RFC 4122

        return new(U128.FromBytesBigEndian(bytes));
    }

    public enum UuidVersion
    {
        Nil,
        V4,
        V7,
        Max,
    }

    /// <summary>
    /// Create a UUIDv7 from a UNIX timestamp (milliseconds) and 10 random bytes.
    /// </summary>
    /// <remarks>
    /// This method will set the variant field within the counter bytes without attempting to shift
    /// the surrounding data. Callers using the counter as a monotonic value should be careful not to
    /// store significant data in the 2 least significant bits of the 3rd byte.
    /// </remarks>
    /// <example>
    /// <code>
    /// var millis = 1686000000000L;
    /// var randomBytes = new byte[10];
    /// var uuid = Uuid.FromUnixMillisV7(millis, randomBytes);
    /// Console.WriteLine(uuid);
    /// // Output: 6e8d8801-005c-0070-8000-000000000000
    /// </code>
    /// </example>
    public static Uuid FromUnixMillisV7(
        long millisSinceUnixEpoch,
        ReadOnlySpan<byte> counterRandomBytes
    )
    {
        if (counterRandomBytes.Length != 10)
        {
            throw new ArgumentException(
                "counterRandomBytes must be exactly 10 bytes",
                nameof(counterRandomBytes)
            );
        }
        // Translated from Rust `uuid`
        var millisHigh = (uint)((millisSinceUnixEpoch >> 16) & 0xFFFF_FFFF);
        var millisLow = (ushort)(millisSinceUnixEpoch & 0xFFFF);

        var counterRandomVersion = (ushort)(
            ((counterRandomBytes[1] | (counterRandomBytes[0] << 8)) & 0x0FFF) | (0x7 << 12)
        ); // version = 7

        Span<byte> d4 =
        [
            // Variant + sequence bytes
            (byte)((counterRandomBytes[2] & 0x3F) | 0x80), // variant: 0b10xxxxxx
            counterRandomBytes[3],
            counterRandomBytes[4],
            counterRandomBytes[5],
            counterRandomBytes[6],
            counterRandomBytes[7],
            counterRandomBytes[8],
            counterRandomBytes[9],
        ];

        // Now assemble UUID bytes exactly like Rust's from_fields
        Span<byte> bytes = stackalloc byte[16];

        // millis_high → bytes[0..4] (big endian)
        bytes[0] = (byte)((millisHigh >> 24) & 0xFF);
        bytes[1] = (byte)((millisHigh >> 16) & 0xFF);
        bytes[2] = (byte)((millisHigh >> 8) & 0xFF);
        bytes[3] = (byte)(millisHigh & 0xFF);

        // millis_low → bytes[4..6] (big endian)
        bytes[4] = (byte)((millisLow >> 8) & 0xFF);
        bytes[5] = (byte)(millisLow & 0xFF);

        // counter_random_version → bytes[6..8] (big endian)
        bytes[6] = (byte)((counterRandomVersion >> 8) & 0xFF);
        bytes[7] = (byte)(counterRandomVersion & 0xFF);

        // d4 → bytes[8..16]
        d4.CopyTo(bytes[8..]);

        return new Uuid(U128.FromBytesBigEndian(bytes));
    }

    /// <summary>
    /// Generate a UUIDv7 using a monotonic <see cref="ClockGenerator"/>.
    /// </summary>
    /// <remarks>
    /// This method will set the variant field within the counter bytes without attempting to shift
    /// the surrounding data. Callers using the counter as a monotonic value should be careful not to
    /// store significant data in the 2 least significant bits of the 3rd byte.
    /// </remarks>
    /// <example>
    /// <code>
    /// var clock = new ClockGenerator(new Timestamp(1686000000000L));
    /// var randomBytes = new byte[10];
    /// var uuid = Uuid.FromClockV7(clock, randomBytes);
    /// Console.WriteLine(uuid);
    /// // Output: 7e640000-8151-0070-8000-000000000000
    /// </code>
    /// </example>
    public static Uuid FromClockV7(ClockGenerator clock, ReadOnlySpan<byte> randomBytes)
    {
        var millis = clock.Tick().ToUnixMillis();
        return FromUnixMillisV7(millis, randomBytes);
    }

    /// <summary>
    /// Returns the <see cref="UuidVersion"/> of this UUID.
    ///
    /// Throws <see cref="InvalidOperationException"/> if the UUID version is unknown.
    /// </summary>
    public UuidVersion GetVersion()
    {
        var bytes = value.ToBytesBigEndian();
        // variant is stored in the 7th byte, in the high nibble.
        var variant = (bytes[6] >> 4) & 0x0F;

        return variant switch
        {
            4 => UuidVersion.V4,
            7 => UuidVersion.V7,

            _ => this == Uuid.NIL ? UuidVersion.Nil
            : this == Uuid.MAX ? UuidVersion.Max
            : throw new InvalidOperationException("Unknown UUID version"),
        };
    }

    private static void GuidToBigEndianBytes(ReadOnlySpan<byte> guidBytes, Span<byte> be)
    {
        // time_low (4 bytes) — little-endian → reverse
        be[0] = guidBytes[3];
        be[1] = guidBytes[2];
        be[2] = guidBytes[1];
        be[3] = guidBytes[0];

        // time_mid (2 bytes) — little-endian
        be[4] = guidBytes[5];
        be[5] = guidBytes[4];

        // time_hi_and_version (2 bytes) — little-endian
        be[6] = guidBytes[7];
        be[7] = guidBytes[6];

        // last 8 bytes already big-endian
        guidBytes[8..].CopyTo(be[8..]);
    }

    private static void BigEndianBytesToGuid(ReadOnlySpan<byte> be, Span<byte> guidBytes)
    {
        // Guid’s weird internal layout (mixed-endian)

        // time_low (4 bytes) — little-endian
        guidBytes[0] = be[3];
        guidBytes[1] = be[2];
        guidBytes[2] = be[1];
        guidBytes[3] = be[0];

        // time_mid (2 bytes) — little-endian
        guidBytes[4] = be[5];
        guidBytes[5] = be[4];

        // time_hi_and_version (2 bytes) — little-endian
        guidBytes[6] = be[7];
        guidBytes[7] = be[6];

        // last 8 bytes already big-endian
        be[8..].CopyTo(guidBytes[8..]);
    }

    public static U128 FromGuid(Guid guid)
    {
        Span<byte> bytes = stackalloc byte[16];
        guid.TryWriteBytes(bytes);

        // Always interpret Guid bytes as big-endian
        return U128.FromBytesBigEndian(bytes);
    }

    /// <summary>
    /// Converts this instance to a <see cref="Guid"/>, in `big-endian`.
    /// </summary>
    public Guid ToGuid()
    {
        // Guid is `mixed-endian`, so we need to fixup
        Span<byte> be = stackalloc byte[16];
        BinaryPrimitives.WriteUInt64BigEndian(be[..8], value.Upper);
        BinaryPrimitives.WriteUInt64BigEndian(be[8..], value.Lower);

        Span<byte> gb = stackalloc byte[16];
        BigEndianBytesToGuid(be, gb);

        return new Guid(gb);
    }

    /// <summary>
    /// Parses a UUID from its string representation.
    /// </summary>
    /// <example>
    /// <code>
    /// var s = "01888d6e-5c00-7000-8000-000000000000";
    /// var uuid = Uuid.Parse(s);
    /// Console.WriteLine(uuid.ToString() == s); // True
    /// </code>
    /// </example>
    public static Uuid Parse(string s)
    {
        // Guid is `mixed-endian`, so we need to fixup
        var guid = new Guid(s);
        Span<byte> gb = stackalloc byte[16];
        guid.TryWriteBytes(gb);

        Span<byte> be = stackalloc byte[16];
        GuidToBigEndianBytes(gb, be);

        return new Uuid(U128.FromBytesBigEndian(be));
    }

    public override readonly string ToString()
    {
        return ToGuid().ToString();
    }

    public readonly int CompareTo(Uuid other) => value.CompareTo(other.value);

    /// <inheritdoc cref="IComparable.CompareTo(object)" />
    public int CompareTo(object? value)
    {
        if (value is Uuid other)
        {
            return CompareTo(other);
        }
        else if (value is null)
        {
            return 1;
        }
        else
        {
            throw new ArgumentException("Argument must be a Uuid", nameof(value));
        }
    }

    public static bool operator <(Uuid l, Uuid r) => l.CompareTo(r) < 0;

    public static bool operator >(Uuid l, Uuid r) => l.CompareTo(r) > 0;

    public readonly partial struct BSATN : IReadWrite<Uuid>
    {
        public Uuid Read(BinaryReader reader) => new(new SpacetimeDB.BSATN.U128Stdb().Read(reader));

        public void Write(BinaryWriter writer, Uuid value) =>
            new SpacetimeDB.BSATN.U128Stdb().Write(writer, value.value);

        // --- / auto-generated ---

        // --- customized ---
        public AlgebraicType GetAlgebraicType(ITypeRegistrar registrar) =>
            // Return a Product directly, not a Ref, because this is a special type.
            new AlgebraicType.Product(
                [
                    // Using this specific name here is important.
                    new("__uuid__", new AlgebraicType.U128(default)),
                ]
            );
    }
}
