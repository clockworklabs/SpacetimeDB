namespace SpacetimeDB;

using System.Buffers.Binary;
using System.Runtime.InteropServices;
using SpacetimeDB.BSATN;

/// <summary>
/// A universally unique identifier (UUID).
///
/// Generate `NIL`, `MAX`, random (`v4`), and time-ordered (`v7`) UUIDs.
/// </summary>
[StructLayout(LayoutKind.Sequential)]
public readonly record struct Uuid : IEquatable<Uuid>, IComparable, IComparable<Uuid>
{
    private readonly U128 value;

    public Uuid(U128 val) => value = val;

    /// <summary>
    /// The nil <see cref="Uuid"/> (all bits set to zero).
    /// </summary>
    public static readonly Uuid NIL = new(new U128());

    /// <summary>
    /// The max <see cref="Uuid"/> (all bits set to one).
    /// </summary>
    public static readonly Uuid MAX = new(new U128(ulong.MaxValue, ulong.MaxValue));

    /// <summary>
    /// Create a <see cref="Uuid"/> `v4` from explicit random bytes.
    /// </summary>
    /// <remarks>
    /// This method assumes the provided bytes are already sufficiently random;
    /// it will only set the appropriate bits for the UUID version and variant.
    /// </remarks>
    /// <param name="randomBytes">
    /// 16 random bytes used for entropy.
    /// </param>
    /// <example>
    /// <code>
    /// var randomBytes = new byte[16];
    /// RandomNumberGenerator.Fill(randomBytes);
    /// var uuid = Uuid.FromRandomBytesV4(randomBytes);
    /// Console.WriteLine(uuid);
    /// // Output: 166a036f-90f3-714a-9731-c8814b70e326
    /// </code>
    /// </example>
    /// <exception cref="ArgumentException">
    /// Thrown if <paramref name="randomBytes"/> is not exactly 16 bytes long.
    /// </exception>
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

    /// <summary>
    /// Generate a <see cref="Uuid"/> `v7` using a monotonic counter from 0 to 2^31-1,
    /// a Unix timestamp in milliseconds, and 4 random bytes.
    ///
    /// <example>
    /// <code>
    /// int counter = 1;
    /// var now = new Timestamp(1_686_000_000_000);
    /// byte[] random = { 0, 0, 0, 0 };
    ///
    /// Guid uuid = UuidV7.FromCounterV7(ref counter, now, random);
    ///
    /// Console.WriteLine(uuid);
    /// // "0000647e-5180-7000-8000-000200000000"
    /// </code>
    /// </example>
    /// </summary>
    /// <remarks>
    ///
    /// The <see cref="Uuid"/> `v7` is structured as follows:
    ///
    /// <code>
    /// ┌───────────────────────────────────────────────┬───────────────────┐
    /// | B0  | B1  | B2  | B3  | B4  | B5              |         B6        |
    /// ├───────────────────────────────────────────────┼───────────────────┤
    /// |                 unix_ts_ms                    |      version 7    |
    /// └───────────────────────────────────────────────┴───────────────────┘
    /// ┌──────────────┬─────────┬──────────────────┬───────────────────────┐
    /// | B7           | B8      | B9  | B10 | B11  | B12 | B13 | B14 | B15 |
    /// ├──────────────┼─────────┼──────────────────┼───────────────────────┤
    /// | counter_high | variant |    counter_low   |        random         |
    /// └──────────────┴─────────┴──────────────────┴───────────────────────┘
    /// </code>
    /// </remarks>
    /// <param name="counter">
    /// Monotonic counter value (31 bits). Must be non-negative.
    /// The counter is incremented and wraps on overflow.
    /// </param>
    /// <param name="now">
    /// Current time.
    /// </param>
    /// <param name="randomBytes">
    /// 4 random bytes used for entropy.
    /// </param>
    /// <exception cref="InvalidOperationException">
    /// Thrown if the counter value is negative.
    /// </exception>
    /// <exception cref="ArgumentException">
    /// Thrown if <paramref name="randomBytes"/> is not exactly 4 bytes long, or <paramref name="now"/> is  before unix epoch.
    /// </exception>
    /// <returns>
    /// A <see cref="Uuid"/> `v7`.
    /// </returns>
    public static Uuid FromCounterV7(
        ref int counter,
        Timestamp now,
        ReadOnlySpan<byte> randomBytes // must be length 4
    )
    {
        if (randomBytes.Length != 4)
        {
            throw new ArgumentException("randomBytes must be exactly 4 bytes", nameof(randomBytes));
        }

        if (counter < 0)
        {
            throw new InvalidOperationException("uuid counter must be non-negative");
        }

        if (now.MicrosecondsSinceUnixEpoch < 0)
        {
            throw new ArgumentException("timestamp before unix epoch", nameof(now));
        }
        var unixTsMs = now.MicrosecondsSinceUnixEpoch / 1_000;

        // monotonic 31-bit
        var counterVal = counter;
        counter = (counter + 1) & 0x7FFF_FFFF;

        Span<byte> bytes = stackalloc byte[16];

        // unix_ts_ms (48 bits, big-endian)
        var ts = unixTsMs & 0x0000_FFFF_FFFF_FFFFL;
        bytes[0] = (byte)(ts >> 40);
        bytes[1] = (byte)(ts >> 32);
        bytes[2] = (byte)(ts >> 24);
        bytes[3] = (byte)(ts >> 16);
        bytes[4] = (byte)(ts >> 8);
        bytes[5] = (byte)ts;

        // version (7)
        bytes[6] = 0x70;

        // counter bits
        bytes[7] = (byte)((counterVal >> 23) & 0xFF);
        bytes[9] = (byte)((counterVal >> 15) & 0xFF);
        bytes[10] = (byte)((counterVal >> 7) & 0xFF);
        bytes[11] = (byte)((counterVal & 0x7F) << 1);

        // variant (RFC 4122)
        bytes[8] = 0x80;

        // random bytes
        bytes[12] |= (byte)(randomBytes[0] & 0x7F);
        bytes[13] = randomBytes[1];
        bytes[14] = randomBytes[2];
        bytes[15] = randomBytes[3];

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
    /// Extract the 31-bit monotonic counter from a <see cref="Uuid"/> `v7`.
    /// Intended for testing.
    /// </summary>
    public int GetCounter()
    {
        var bytes = value.ToBytesBigEndian();

        uint high = bytes[7];
        uint mid1 = bytes[9];
        uint mid2 = bytes[10];
        var low = (uint)(bytes[11] >> 1);

        // Reconstruct 31-bit counter
        return (int)((high << 23) | (mid1 << 15) | (mid2 << 7) | low);
    }

    /// <summary>
    /// Returns the <see cref="UuidVersion"/> of this <see cref="Uuid"/>.
    ///
    /// Throws <see cref="InvalidOperationException"/> if the <see cref="Uuid"/> version is unknown.
    /// </summary>
    public UuidVersion GetVersion()
    {
        var bytes = value.ToBytesBigEndian();
        var version = (bytes[6] >> 4) & 0x0F;

        return version switch
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
        // Guid’s weird internal layout (mixed-endian)

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

    public static Uuid FromGuid(Guid guid)
    {
        // Guid is `mixed-endian`, so we need to fixup
        Span<byte> gb = stackalloc byte[16];
        guid.TryWriteBytes(gb);

        Span<byte> bytes = stackalloc byte[16];
        GuidToBigEndianBytes(gb, bytes);

        return new Uuid(U128.FromBytesBigEndian(bytes));
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
        GuidToBigEndianBytes(be, gb);

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
        var guid = new Guid(s);

        return Uuid.FromGuid(guid);
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
