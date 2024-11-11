namespace SpacetimeDB;

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

        byte[] bytes =
        [
            0x00,
            0x11,
            0x22,
            0x33,
            0x44,
            0x55,
            0x66,
            0x77,
            0x88,
            0x99,
            0xaa,
            0xbb,
            0xcc,
            0xdd,
            0xee,
            0xff,
        ];
        var addr2 = Address.FromBigEndian(bytes);
        Assert.Equal(addr2, addr);

        Array.Reverse(bytes);
        var addr3 = Address.From(bytes);
        Assert.Equal(addr3, addr);

        var memoryStream = new MemoryStream();
        var writer = new BinaryWriter(memoryStream);
        var bsatn = new Address.BSATN();
        if (addr is { } addrNotNull)
        {
            bsatn.Write(writer, addrNotNull);
        }
        else
        {
            Assert.Fail("Impossible");
        }
        writer.Flush();

        var littleEndianBytes = memoryStream.ToArray();
        var reader = new BinaryReader(new MemoryStream(littleEndianBytes));
        var addr4 = bsatn.Read(reader);
        Assert.Equal(addr4, addr);

        // Note: From = FromLittleEndian
        var addr5 = Address.From(littleEndianBytes);
        Assert.Equal(addr5, addr);
    }

    [Fact]
    public static void AddressLengthCheck()
    {
        for (var i = 0; i < 64; i++)
        {
            if (i == 16)
            {
                continue;
            }

            var bytes = new byte[i];

            Assert.ThrowsAny<Exception>(() => Address.From(bytes));
        }
    }

    [Fact]
    public static void IdentityRoundtrips()
    {
        var str = "00112233445566778899AABBCCDDEEFF00112233445566778899AABBCCDDEEFF";
        var ident = Identity.FromHexString(str);

        Assert.Equal(ident.ToString(), str);

        byte[] bytes =
        [
            0x00,
            0x11,
            0x22,
            0x33,
            0x44,
            0x55,
            0x66,
            0x77,
            0x88,
            0x99,
            0xaa,
            0xbb,
            0xcc,
            0xdd,
            0xee,
            0xff,
            0x00,
            0x11,
            0x22,
            0x33,
            0x44,
            0x55,
            0x66,
            0x77,
            0x88,
            0x99,
            0xaa,
            0xbb,
            0xcc,
            0xdd,
            0xee,
            0xff,
        ];
        var ident2 = Identity.FromBigEndian(bytes);
        Assert.Equal(ident2, ident);

        Array.Reverse(bytes);
        var ident3 = Identity.From(bytes);
        Assert.Equal(ident3, ident);

        var memoryStream = new MemoryStream();
        var writer = new BinaryWriter(memoryStream);
        var bsatn = new Identity.BSATN();
        bsatn.Write(writer, ident);
        writer.Flush();

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
        for (var i = 0; i < 64; i++)
        {
            if (i == 32)
            {
                continue;
            }

            var bytes = new byte[i];

            Assert.ThrowsAny<Exception>(() => Identity.From(bytes));
        }
    }
}
