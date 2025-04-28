namespace SpacetimeDB;

using CsCheck;
using Xunit;

public static partial class BSATNRuntimeTests
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

    [Fact]
    public static void ConnectionIdComparableChecks()
    {
        var str = "00112233445566778899AABBCCDDEEFF";
        var strHigh = "00001111222233334444555566667777";
        var strLow = "FFFFEEEEDDDDCCCCBBBBAAAA99998888";

        var connIdA = ConnectionId.FromHexString(str);
        var connIdB = ConnectionId.FromHexString(str);
        var connIdHigh = ConnectionId.FromHexString(strHigh);
        var connIdLow = ConnectionId.FromHexString(strLow);

        Assert.NotNull(connIdA);
        Assert.NotNull(connIdB);
        Assert.NotNull(connIdHigh);
        Assert.NotNull(connIdLow);

        Assert.Equal(0, connIdA.Value.CompareTo(connIdB.Value));
        Assert.Equal(+1, connIdA.Value.CompareTo(connIdHigh.Value));
        Assert.Equal(-1, connIdA.Value.CompareTo(connIdLow.Value));

        var notAConnId = new uint();

        Assert.ThrowsAny<Exception>(() => connIdA.Value.CompareTo(notAConnId));
    }

    [Fact]
    public static void IdentityComparableChecks()
    {
        var str = "00112233445566778899AABBCCDDEEFF00112233445566778899AABBCCDDEEFF";
        var strHigh = "0000111122223333444455556666777788889999AAAABBBBCCCCDDDDEEEEFFFF";
        var strLow = "FFFFEEEEDDDDCCCCBBBBAAAA9999888877776666555544443333222211110000";

        var identityA = Identity.FromHexString(str);
        var identityB = Identity.FromHexString(str);
        var identityHigh = Identity.FromHexString(strHigh);
        var identityLow = Identity.FromHexString(strLow);

        Assert.Equal(0, identityA.CompareTo(identityB));
        Assert.Equal(+1, identityA.CompareTo(identityHigh));
        Assert.Equal(-1, identityA.CompareTo(identityLow));

        var notAnIdentity = new uint();

        Assert.ThrowsAny<Exception>(() => identityA.CompareTo(notAnIdentity));
    }

    [Type]
    public partial class BasicDataClass
    {
        public int X;
        public string Y = "";
        public int? Z;
        public string? W;

        public BasicDataClass() { }

        public BasicDataClass((int x, string y, int? z, string? w) data)
        {
            X = data.x;
            Y = data.y;
            Z = data.z;
            W = data.w;
        }
    }

    [Type]
    public partial struct BasicDataStruct
    {
        public int X;
        public string Y;
        public int? Z;
        public string? W;

        public BasicDataStruct((int x, string y, int? z, string? w) data)
        {
            X = data.x;
            Y = data.y;
            Z = data.z;
            W = data.w;
        }
    }

    [Type]
    public partial record BasicDataRecord
    {
        public int X;
        public string Y = "";
        public int? Z;
        public string? W;

        public BasicDataRecord() { }

        public BasicDataRecord((int x, string y, int? z, string? w) data)
        {
            X = data.x;
            Y = data.y;
            Z = data.z;
            W = data.w;
        }
    }

    static readonly Gen<int> GenSmallInt = Gen.Int[-5, 5];
    static readonly Gen<string> GenSmallString = Gen.String[Gen.Char.AlphaNumeric, 0, 2];
    static readonly Gen<int?> GenNullableInt = Gen.Nullable<int>(GenSmallInt);
    static readonly Gen<string?> GenNullableString = Gen.Null<string>(GenSmallString);

    static readonly Gen<(int X, string Y, int? Z, string? W)> GenBasic = Gen.Select(
        GenSmallInt,
        GenSmallString,
        GenNullableInt,
        GenNullableString,
        (x, y, z, w) => (x, y, z, w)
    );
    static readonly Gen<(
        (int X, string Y, int? Z, string? W) c1,
        (int X, string Y, int? Z, string? W) c2
    )> GenTwoBasic = Gen.Select(GenBasic, GenBasic, (c1, c2) => (c1, c2));

    [Fact]
    public static void TestGeneratedEquals()
    {
        GenTwoBasic.Sample(
            example =>
            {
                var class1 = new BasicDataClass(example.c1);
                var class2 = new BasicDataClass(example.c2);

                var struct1 = new BasicDataStruct(example.c1);
                var struct2 = new BasicDataStruct(example.c2);

                var record1 = new BasicDataRecord(example.c1);
                var record2 = new BasicDataRecord(example.c2);

                if (example.c1 == example.c2)
                {
                    Assert.Equal(class1, class2);
                    Assert.True(class1 == class2);
                    Assert.False(class1 != class2);
                    Assert.Equal(class1.ToString(), class2.ToString());
                    Assert.Equal(class1.GetHashCode(), class2.GetHashCode());

                    Assert.Equal(struct1, struct2);
                    Assert.True(struct1 == struct2);
                    Assert.False(struct1 != struct2);
                    Assert.Equal(struct1.ToString(), struct2.ToString());
                    Assert.Equal(struct1.GetHashCode(), struct2.GetHashCode());

                    Assert.Equal(record1, record2);
                    Assert.True(record1 == record2);
                    Assert.False(record1 != record2);
                    Assert.Equal(record1.ToString(), record2.ToString());
                    Assert.Equal(record1.GetHashCode(), record2.GetHashCode());

                    // hash code should not depend on the type of object.
                    Assert.Equal(class1.GetHashCode(), record1.GetHashCode());
                    Assert.Equal(record1.GetHashCode(), struct1.GetHashCode());
                }
                else
                {
                    Assert.NotEqual(class1, class2);
                    Assert.False(class1 == class2);
                    Assert.True(class1 != class2);
                    Assert.NotEqual(class1.ToString(), class2.ToString());

                    Assert.NotEqual(struct1, struct2);
                    Assert.False(struct1 == struct2);
                    Assert.True(struct1 != struct2);
                    Assert.NotEqual(struct1.ToString(), struct2.ToString());

                    Assert.NotEqual(record1, record2);
                    Assert.False(record1 == record2);
                    Assert.True(record1 != record2);
                    Assert.NotEqual(record1.ToString(), record2.ToString());

                    // hash code should not depend on the type of object.
                    Assert.Equal(class1.GetHashCode(), record1.GetHashCode());
                    Assert.Equal(record1.GetHashCode(), struct1.GetHashCode());
                }
            },
            iter: 10_000
        );
    }

    [Type]
    public partial record BasicEnum
        : TaggedEnum<(
            int X,
            string Y,
            int? Z,
            string? T,
            BasicDataClass U,
            BasicDataStruct V,
            BasicDataRecord W
        )> { }

    static readonly Gen<BasicEnum> GenBasicEnum = Gen.SelectMany<int, BasicEnum>(
        Gen.Int[0, 7],
        tag =>
        {
            return tag switch
            {
                0 => GenSmallInt.Select(v => new BasicEnum.X(v)),
                1 => GenSmallString.Select(v => new BasicEnum.Y(v)),
                2 => GenNullableInt.Select(v => new BasicEnum.Z(v)),
                3 => GenNullableString.Select(v => new BasicEnum.T(v)),
                4 => GenBasic.Select(v => new BasicEnum.U(new BasicDataClass(v))),
                5 => GenBasic.Select(v => new BasicEnum.V(new BasicDataStruct(v))),
                _ => GenBasic.Select(v => new BasicEnum.W(new BasicDataRecord(v))),
            };
        }
    );
    static readonly Gen<(BasicEnum e1, BasicEnum e2)> GenTwoBasicEnum = Gen.Select(
        GenBasicEnum,
        GenBasicEnum,
        (e1, e2) => (e1, e2)
    );

    [Type]
    public partial class ContainsList
    {
        public List<BasicEnum?> TheList = [];

        public ContainsList() { }

        public ContainsList(List<BasicEnum?> theList)
        {
            TheList = theList;
        }
    }

    [Fact]
    public static void GeneratedEnumsWork()
    {
        GenTwoBasicEnum.Sample(
            example =>
            {
                var equal = example switch
                {
                    (BasicEnum.X(var v1), BasicEnum.X(var v2)) => v1.Equals(v2),
                    (BasicEnum.Y(var v1), BasicEnum.Y(var v2)) => v1.Equals(v2),
                    (BasicEnum.Z(var v1), BasicEnum.Z(var v2)) => v1.Equals(v2),
                    (BasicEnum.T(var v1), BasicEnum.T(var v2)) => v1 == null
                        ? v2 == null
                        : v1.Equals(v2),
                    (BasicEnum.U(var v1), BasicEnum.U(var v2)) => v1.Equals(v2),
                    (BasicEnum.V(var v1), BasicEnum.V(var v2)) => v1.Equals(v2),
                    (BasicEnum.W(var v1), BasicEnum.W(var v2)) => v1.Equals(v2),
                    _ => false,
                };

                if (equal)
                {
                    Assert.Equal(example.e1, example.e2);
                    Assert.True(example.e1 == example.e2);
                    Assert.False(example.e1 != example.e2);
                    Assert.Equal(example.e1.ToString(), example.e2.ToString());
                    Assert.Equal(example.e1.GetHashCode(), example.e2.GetHashCode());
                }
                else
                {
                    Assert.NotEqual(example.e1, example.e2);
                    Assert.False(example.e1 == example.e2);
                    Assert.True(example.e1 != example.e2);
                    Assert.NotEqual(example.e1.ToString(), example.e2.ToString());
                }
            },
            iter: 10_000
        );
    }

    [Fact]
    public static void GeneratedToString()
    {
        Assert.Equal("\"\"", BSATN.StringUtil.GenericToString(""));
        Assert.Equal("null", BSATN.StringUtil.GenericToString(null));
        Assert.Equal("[ ]", BSATN.StringUtil.GenericToString(new List<int?> { }));
        Assert.Equal(
            "[ null, null, 3 ]",
            BSATN.StringUtil.GenericToString(new List<int?> { null, null, 3 })
        );
        Assert.Equal(
            "[ null, null, \"hi\" ]",
            BSATN.StringUtil.GenericToString(new List<string?> { null, null, "hi" })
        );
        Assert.Equal(
            "[ null, null, X(1) ]",
            BSATN.StringUtil.GenericToString(
                new List<BasicEnum?> { null, null, new BasicEnum.X(1) }
            )
        );
        Assert.Equal(
            "[ 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15 ]",
            BSATN.StringUtil.GenericToString(
                new List<int?> { 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15 }
            )
        );
        Assert.Equal(
            "[ 0, 1, 2, 3, 4, 5, 6, 7, ..., 9, 10, 11, 12, 13, 14, 15, 16 ]",
            BSATN.StringUtil.GenericToString(
                new List<int?> { 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16 }
            )
        );
        Assert.Equal(
            "[ 0, 1, 2, 3, 4, 5, 6, 7, ..., 10, 11, 12, 13, 14, 15, 16, 17 ]",
            BSATN.StringUtil.GenericToString(
                new List<int?> { 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17 }
            )
        );
        Assert.Equal("X(1)", new BasicEnum.X(1).ToString());
        Assert.Equal("Y(\"hi\")", new BasicEnum.Y("hi").ToString());
        Assert.Equal("Y(null)", new BasicEnum.Y(null!).ToString());
        Assert.Equal("Z(1)", new BasicEnum.Z(1).ToString());
        Assert.Equal("Z(null)", new BasicEnum.Z(null).ToString());
        Assert.Equal("T(null)", new BasicEnum.T(null).ToString());
        Assert.Equal("T(\"\")", new BasicEnum.T("").ToString());
        // There is unfortunately some stuttering if the variant and the stored data have the same name. Shrug.
        Assert.Equal(
            "U(BasicDataClass { X = 1, Y = \"hi\", Z = null, W = null })",
            new BasicEnum.U(new BasicDataClass((1, "hi", null, null))).ToString()
        );
        Assert.Equal(
            "V(BasicDataStruct { X = 1, Y = \"hi\", Z = null, W = null })",
            new BasicEnum.V(new BasicDataStruct((1, "hi", null, null))).ToString()
        );
        Assert.Equal(
            "W(BasicDataRecord { X = 1, Y = \"hi\", Z = null, W = null })",
            new BasicEnum.W(new BasicDataRecord((1, "hi", null, null))).ToString()
        );
        Assert.Equal(
            "ContainsList { TheList = [ X(1), Y(\"hi\"), W(BasicDataRecord { X = 1, Y = \"hi\", Z = null, W = null }) ] }",
            new ContainsList(
                [
                    new BasicEnum.X(1),
                    new BasicEnum.Y("hi"),
                    new BasicEnum.W(new BasicDataRecord((1, "hi", null, null))),
                ]
            ).ToString()
        );
    }
}
