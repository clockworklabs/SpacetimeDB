namespace SpacetimeDB;

using CsCheck;
using Xunit;

public static class BSATNRuntimeTests
{
    [Fact]
    public static void ConnectionIdRoundtrips()
    {
        var str = "00112233445566778899AABBCCDDEEFF";
        var connId = ConnectionId.FromHexString(str);

        Assert.NotNull(connId);
        Assert.Equal(connId.ToString(), str);

        var bytes = Convert.FromHexString(str);

        var connId2 = ConnectionId.FromBigEndian(bytes);
        Assert.Equal(connId2, connId);

        Array.Reverse(bytes);
        var connId3 = ConnectionId.From(bytes);
        Assert.Equal(connId3, connId);

        var memoryStream = new MemoryStream();
        var bsatn = new ConnectionId.BSATN();
        using (var writer = new BinaryWriter(memoryStream))
        {
            if (connId is { } connIdNotNull)
            {
                bsatn.Write(writer, connIdNotNull);
            }
            else
            {
                Assert.Fail("Impossible");
            }
        }

        var littleEndianBytes = memoryStream.ToArray();
        var reader = new BinaryReader(new MemoryStream(littleEndianBytes));
        var connId4 = bsatn.Read(reader);
        Assert.Equal(connId4, connId);

        // Note: From = FromLittleEndian
        var connId5 = ConnectionId.From(littleEndianBytes);
        Assert.Equal(connId5, connId);
    }

    static readonly Gen<string> genHex = Gen.String[Gen.Char["0123456789abcdef"], 0, 128];

    [Fact]
    public static void ConnectionIdLengthCheck()
    {
        genHex.Sample(s =>
        {
            if (s.Length == 32)
            {
                return;
            }
            Assert.ThrowsAny<Exception>(() => ConnectionId.FromHexString(s));
        });
        Gen.Byte.Array[0, 64]
            .Sample(arr =>
            {
                if (arr.Length == 16)
                {
                    return;
                }
                Assert.ThrowsAny<Exception>(() => ConnectionId.FromBigEndian(arr));
                Assert.ThrowsAny<Exception>(() => ConnectionId.From(arr));
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
            () => ConnectionId.FromHexString("these are not hex characters....")
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

#pragma warning disable CS1718
        Assert.True(stamp == stamp);
#pragma warning restore CS1718
        Assert.False(stamp == laterStamp);
        Assert.True(stamp < laterStamp);
        Assert.False(laterStamp < stamp);
        Assert.Equal(-1, stamp.CompareTo(laterStamp));
        Assert.Equal(+1, laterStamp.CompareTo(stamp));
    }
}
