namespace SpacetimeDB;

using CsCheck;
using Xunit;

public static class BSATNRuntimeTests
{
    [Fact]
    public static void AddressRoundtrips()
    {
        var str = "00112233445566778899AABBCCDDEEFF";
        var addr = Address.FromHexString(str);

        Assert.NotNull(addr);
        Assert.Equal(addr.ToString(), str);

        var bytes = Convert.FromHexString(str);

        var addr2 = Address.FromBigEndian(bytes);
        Assert.Equal(addr2, addr);

        Array.Reverse(bytes);
        var addr3 = Address.From(bytes);
        Assert.Equal(addr3, addr);

        var memoryStream = new MemoryStream();
        var bsatn = new Address.BSATN();
        using (var writer = new BinaryWriter(memoryStream))
        {
            if (addr is { } addrNotNull)
            {
                bsatn.Write(writer, addrNotNull);
            }
            else
            {
                Assert.Fail("Impossible");
            }
        }

        var littleEndianBytes = memoryStream.ToArray();
        var reader = new BinaryReader(new MemoryStream(littleEndianBytes));
        var addr4 = bsatn.Read(reader);
        Assert.Equal(addr4, addr);

        // Note: From = FromLittleEndian
        var addr5 = Address.From(littleEndianBytes);
        Assert.Equal(addr5, addr);
    }

    static readonly Gen<string> genHex = Gen.String[Gen.Char["0123456789abcdef"], 0, 128];

    [Fact]
    public static void AddressLengthCheck()
    {
        genHex.Sample(s =>
        {
            if (s.Length == 32)
            {
                return;
            }
            Assert.ThrowsAny<Exception>(() => Address.FromHexString(s));
        });
        Gen.Byte.Array[0, 64]
            .Sample(arr =>
            {
                if (arr.Length == 16)
                {
                    return;
                }
                Assert.ThrowsAny<Exception>(() => Address.FromBigEndian(arr));
                Assert.ThrowsAny<Exception>(() => Address.From(arr));
            });
    }

    [Fact]
    public static void IdentityRoundtrips()
    {
        var str = "00112233445566778899AABBCCDDEEFF00112233445566778899AABBCCDDEEFF";
        var ident = Identity.FromHexString(str);

        Assert.Equal(ident.ToString(), str);

        // We can't use this in the implementation because it isn't available
        // in Unity's .NET. But we can use it in tests.
        var bytes = Convert.FromHexString(str);

        var ident2 = Identity.FromBigEndian(bytes);
        Assert.Equal(ident2, ident);

        Array.Reverse(bytes);
        var ident3 = Identity.From(bytes);
        Assert.Equal(ident3, ident);

        var memoryStream = new MemoryStream();
        var bsatn = new Identity.BSATN();
        using (var writer = new BinaryWriter(memoryStream))
        {
            bsatn.Write(writer, ident);
        }

        var littleEndianBytes = memoryStream.ToArray();
        var reader = new BinaryReader(new MemoryStream(littleEndianBytes));
        var ident4 = bsatn.Read(reader);
        Assert.Equal(ident4, ident);

        // Note: From = FromLittleEndian
        var ident5 = Identity.From(littleEndianBytes);
        Assert.Equal(ident5, ident);
    }

    [Fact]
    public static void IdentityLengthCheck()
    {
        genHex.Sample(s =>
        {
            if (s.Length == 64)
            {
                return;
            }
            Assert.ThrowsAny<Exception>(() => Identity.FromHexString(s));
        });
        Gen.Byte.Array[0, 64]
            .Sample(arr =>
            {
                if (arr.Length == 32)
                {
                    return;
                }
                Assert.ThrowsAny<Exception>(() => Identity.FromBigEndian(arr));
                Assert.ThrowsAny<Exception>(() => Identity.From(arr));
            });
    }

    [Fact]
    public static void NonHexStrings()
    {
        // n.b. 32 chars long
        Assert.ThrowsAny<Exception>(
            () => Address.FromHexString("these are not hex characters....")
        );
    }

    [Fact]
    public static void TimestampConversionChecks()
    {
        var us = 1737582793990639L;

        var time = ScheduleAt.DateTimeOffsetFromMicrosSinceUnixEpoch(us);
        Assert.Equal(ScheduleAt.ToMicrosecondsSinceUnixEpoch(time), us);

        var interval = ScheduleAt.TimeSpanFromMicroseconds(us);
        Assert.Equal(ScheduleAt.ToMicroseconds(interval), us);

        var stamp = new Timestamp(us);
        var dto = (DateTimeOffset)stamp;
        var stamp_ = (Timestamp)dto;
        Assert.Equal(stamp, stamp_);

        var duration = new TimeDuration(us);
        var timespan = (TimeSpan)duration;
        var duration_ = (TimeDuration)timespan;
        Assert.Equal(duration, duration_);

        var newIntervalUs = 333L;
        var newInterval = new TimeDuration(newIntervalUs);
        var laterStamp = stamp + newInterval;
        Assert.Equal(laterStamp.MicrosecondsSinceUnixEpoch, us + newIntervalUs);
        Assert.Equal(laterStamp.TimeDurationSince(stamp), newInterval);
    }
}
