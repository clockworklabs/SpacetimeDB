namespace SpacetimeDB;

using System.Diagnostics.CodeAnalysis;
using System.Security.Cryptography;
using CsCheck;
using SpacetimeDB.BSATN;
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
    public partial struct BasicDataStruct((int x, string y, int? z, string? w) data)
    {
        public int X = data.x;
        public string Y = data.y;
        public int? Z = data.z;
        public string? W = data.w;
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

    /// <summary>
    /// Count collisions when comparing hashcodes of non-equal structures.
    /// </summary>
    struct CollisionCounter
    {
        private uint Comparisons;
        private uint Collisions;

        public void Add(bool collides)
        {
            Comparisons += 1;
            if (collides)
            {
                Collisions += 1;
            }
        }

        public readonly double CollisionFraction
        {
            get => (double)Collisions / (double)Comparisons;
        }

        public readonly void AssertCollisionsLessThan(double fraction)
        {
            Assert.True(
                CollisionFraction < fraction,
                $"Expected {fraction} portion of collisions, but got {CollisionFraction} = {Collisions} / {Comparisons}"
            );
        }
    }

    static void TestRoundTrip<T, BSATN>(Gen<T> gen, BSATN serializer)
        where BSATN : IReadWrite<T>
    {
        gen.Sample(
            (value) =>
            {
                var stream = new MemoryStream();
                var writer = new BinaryWriter(stream);
                serializer.Write(writer, value);
                stream.Seek(0, SeekOrigin.Begin);
                var reader = new BinaryReader(stream);
                var result = serializer.Read(reader);
                Assert.Equal(value, result);
            },
            iter: 10_000
        );
    }

    [Fact]
    public static void GeneratedProductRoundTrip()
    {
        TestRoundTrip(
            GenBasic.Select(value => new BasicDataClass(value)),
            new BasicDataClass.BSATN()
        );
        TestRoundTrip(
            GenBasic.Select(value => new BasicDataRecord(value)),
            new BasicDataRecord.BSATN()
        );
        TestRoundTrip(
            GenBasic.Select(value => new BasicDataStruct(value)),
            new BasicDataStruct.BSATN()
        );
    }

    [Fact]
    public static void GeneratedProductEqualsWorks()
    {
        CollisionCounter collisionCounter = new();

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

                    collisionCounter.Add(class1.GetHashCode() == class2.GetHashCode());
                }
            },
            iter: 10_000
        );
        collisionCounter.AssertCollisionsLessThan(0.05);
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

    [Fact]
    public static void GeneratedSumRoundTrip()
    {
        TestRoundTrip(GenBasicEnum, new BasicEnum.BSATN());
    }

    [Fact]
    public static void GeneratedSumEqualsWorks()
    {
        CollisionCounter collisionCounter = new();

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
                    collisionCounter.Add(example.e1.GetHashCode() == example.e2.GetHashCode());
                }
            },
            iter: 10_000
        );
        collisionCounter.AssertCollisionsLessThan(0.05);
    }

    [Type]
    public partial class ContainsList
    {
        public List<BasicEnum?>? TheList = [];

        public ContainsList() { }

        public ContainsList(List<BasicEnum?>? theList)
        {
            TheList = theList;
        }
    }

    static readonly Gen<ContainsList> GenContainsList = GenBasicEnum
        .Null()
        .List[0, 2]
        .Null()
        .Select(list => new ContainsList(list));
    static readonly Gen<(ContainsList e1, ContainsList e2)> GenTwoContainsList = Gen.Select(
        GenContainsList,
        GenContainsList,
        (e1, e2) => (e1, e2)
    );

    [Fact]
    public static void GeneratedListRoundTrip()
    {
        TestRoundTrip(GenContainsList, new ContainsList.BSATN());
    }

    [Fact]
    public static void GeneratedListEqualsWorks()
    {
        CollisionCounter collisionCounter = new();
        GenTwoContainsList.Sample(
            example =>
            {
                var equal =
                    example.e1.TheList == null
                        ? example.e2.TheList == null
                        : (
                            example.e2.TheList == null
                                ? false
                                : example.e1.TheList.SequenceEqual(example.e2.TheList)
                        );

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
                    collisionCounter.Add(example.e1.GetHashCode() == example.e2.GetHashCode());
                }
            },
            iter: 10_000
        );
        collisionCounter.AssertCollisionsLessThan(0.05);
    }

    [Type]
    public partial class ContainsNestedList
    {
        public List<BasicEnum[][]> TheList = [];

        public ContainsNestedList() { }

        public ContainsNestedList(List<BasicEnum[][]> theList)
        {
            TheList = theList;
        }
    }

    // For the serialization test, forbid nulls.
    static readonly Gen<ContainsNestedList> GenContainsNestedListNoNulls = GenBasicEnum
        .Array[0, 2]
        .Array[0, 2]
        .List[0, 2]
        .Select(list => new ContainsNestedList(list));

    [Fact]
    public static void GeneratedNestedListRoundTrip()
    {
        TestRoundTrip(GenContainsNestedListNoNulls, new ContainsNestedList.BSATN());
    }

    // However, for the equals + hashcode test, throw in some nulls, just to be paranoid.
    // The user might have constructed a bad one of these in-memory.

#pragma warning disable CS8620 // Argument cannot be used for parameter due to differences in the nullability of reference types.
    static readonly Gen<ContainsNestedList> GenContainsNestedList = GenBasicEnum
        .Null()
        .Array[0, 2]
        .Null()
        .Array[0, 2]
        .Null()
        .List[0, 2]
        .Select(list => new ContainsNestedList(list));
#pragma warning restore CS8620 // Argument cannot be used for parameter due to differences in the nullability of reference types.

    static readonly Gen<(ContainsNestedList e1, ContainsNestedList e2)> GenTwoContainsNestedList =
        Gen.Select(GenContainsNestedList, GenContainsNestedList, (e1, e2) => (e1, e2));

    class EnumerableEqualityComparer<T>(EqualityComparer<T> equalityComparer)
        : EqualityComparer<IEnumerable<T>>
    {
        private readonly EqualityComparer<T> EqualityComparer = equalityComparer;

        public override bool Equals(IEnumerable<T>? x, IEnumerable<T>? y) =>
            x == null ? y == null : (y == null ? false : x.SequenceEqual(y, EqualityComparer));

        public override int GetHashCode([DisallowNull] IEnumerable<T> obj)
        {
            var hashCode = 0;
            foreach (var item in obj)
            {
                if (item != null)
                {
                    hashCode ^= EqualityComparer.GetHashCode(item);
                }
            }
            return hashCode;
        }
    }

    [Type]
    enum Banana
    {
        Cavendish,
        LadyFinger,
        RedBanana,
        Manzano,
        BlueJava,
        GreenPlantain,
        YellowPlantain,
        PisangRaja,
    }

    [Fact]
    public static void EnumSerializationWorks()
    {
        var serializer = new Enum<Banana>();
        var bananas = new Banana[]
        {
            Banana.Cavendish,
            Banana.LadyFinger,
            Banana.RedBanana,
            Banana.Manzano,
            Banana.BlueJava,
            Banana.GreenPlantain,
            Banana.YellowPlantain,
            Banana.PisangRaja,
        };
        for (var i = 0; i < bananas.Length; i++)
        {
            var stream = new MemoryStream();
            var writer = new BinaryWriter(stream);
            var banana = bananas[i];
            serializer.Write(writer, banana);

            stream.Seek(0, SeekOrigin.Begin);
            var tag = new BinaryReader(stream).ReadByte();
            Assert.Equal(tag, i);

            stream.Seek(0, SeekOrigin.Begin);
            var newBanana = serializer.Read(new BinaryReader(stream));
            Assert.Equal(banana, newBanana);
        }
    }

    [Fact]
    public static void GeneratedNestedListEqualsWorks()
    {
        var equalityComparer = new EnumerableEqualityComparer<IEnumerable<IEnumerable<BasicEnum>>>(
            new EnumerableEqualityComparer<IEnumerable<BasicEnum>>(
                new EnumerableEqualityComparer<BasicEnum>(EqualityComparer<BasicEnum>.Default)
            )
        );
        CollisionCounter collisionCounter = new();
        GenTwoContainsNestedList.Sample(
            example =>
            {
                var equal = equalityComparer.Equals(example.e1.TheList, example.e2.TheList);

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
                    collisionCounter.Add(example.e1.GetHashCode() == example.e2.GetHashCode());
                }
            },
            iter: 10_000
        );
        collisionCounter.AssertCollisionsLessThan(0.05);
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
#pragma warning disable CS8625 // Cannot convert null literal to non-nullable reference type.
        Assert.Equal(
            "ContainsNestedList { TheList = [ [ [ X(1), null ], null ], null ] }",
            new ContainsNestedList(
                [
                    [
                        [new BasicEnum.X(1), null],
                        null,
                    ],
                    null,
                ]
            ).ToString()
        );
#pragma warning restore CS8625 // Cannot convert null literal to non-nullable reference type.
    }

    [Fact]
    public static void NonNullableStringSerializationRejectsNull()
    {
        var stream = new MemoryStream();
        var writer = new BinaryWriter(stream);

        var serializer = new BSATN.String();
        var ex = Assert.Throws<ArgumentNullException>(() => serializer.Write(writer, null!));
        Assert.Contains("nullable string", ex.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public static void NullableStringOptionRoundTripsNull()
    {
        var stream = new MemoryStream();
        var writer = new BinaryWriter(stream);
        var serializer = new BSATN.RefOption<string, BSATN.String>();

        serializer.Write(writer, null);

        stream.Seek(0, SeekOrigin.Begin);
        var reader = new BinaryReader(stream);
        var value = serializer.Read(reader);
        Assert.Null(value);
    }

    [Type]
    partial struct ContainsEnum
    {
        public Banana TheBanana;
        public int BananaCount;
    }

    static readonly Gen<(Banana, int)> GenContainsEnum = Gen.Select(
        Gen.Enum<Banana>(),
        Gen.Int[0, 3]
    );
    static readonly Gen<((Banana, int), (Banana, int))> GenTwoContainsEnum = Gen.Select(
        GenContainsEnum,
        GenContainsEnum
    );

    [Fact]
    public static void GeneratedEnumEqualsWorks()
    {
        GenTwoContainsEnum.Sample(
            example =>
            {
                var ((b1, c1), (b2, c2)) = example;
                var struct1 = new ContainsEnum { TheBanana = b1, BananaCount = c1 };
                var struct2 = new ContainsEnum { TheBanana = b2, BananaCount = c2 };

                if ((b1, c1) == (b2, c2))
                {
                    Assert.True(struct1.Equals(struct2));
                    Assert.Equal(struct1, struct2);
                }
                else
                {
                    Assert.False(struct1.Equals(struct2));
                    Assert.NotEqual(struct1, struct2);
                }
            },
            iter: 10_000
        );
    }

    [Fact]
    public static void UUidRoundTrip()
    {
        var u1 = Uuid.NIL;
        var s = u1.ToString();
        var u2 = Uuid.Parse(s);
        Assert.Equal(u1, u2);
        Assert.Equal(u1.ToGuid(), u2.ToGuid());
        Assert.Equal(s, u2.ToString());
    }

    [Fact]
    public static void UuidToString()
    {
        foreach (
            var uuid in new[]
            {
                Uuid.NIL,
                new Uuid(new U128(0x0102030405060708UL, 0x090A0B0C0D0E0F10UL)),
                Uuid.MAX,
            }
        )
        {
            var s = uuid.ToString();
            var uuid2 = Uuid.Parse(s);
            Assert.Equal(uuid, uuid2);
            var g = new Guid(s);
            Assert.Equal(s, g.ToString()); // same canonical form
        }
    }

    [Fact]
    public static void WrapAround()
    {
        // Check wraparound behavior
        var counter = int.MaxValue;
        var ts = Timestamp.UNIX_EPOCH;

        _ = Uuid.FromCounterV7(ref counter, ts, new byte[4]);

        Assert.Equal(0, counter);
    }

    [Fact]
    public static void NegativeTimestampThrows()
    {
        var counter = 0;
        var ts = new Timestamp(-1);

        var ex = Assert.Throws<ArgumentException>(
            () => Uuid.FromCounterV7(ref counter, ts, new byte[4])
        );

        Assert.Contains("timestamp", ex.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public static void UuidOrdered()
    {
        var u1 = new Uuid(new U128(1, 0));
        var u2 = new Uuid(new U128(2, 0));

        Assert.True(u1 < u2);
        Assert.True(u2 > u1);
        Assert.Equal(u1, u1);
        Assert.NotEqual(u1, u2);

        // Check we start from zero
        var counter = 0;
        var ts = Timestamp.UNIX_EPOCH;

        var uStart = Uuid.FromCounterV7(ref counter, ts, new byte[4]);
        Assert.Equal(0, uStart.GetCounter());

        // Check ordering across many UUIDs
        const int total = 10_000_000;
        counter = int.MaxValue - total;

        var uuids = Enumerable
            .Range(0, 1000)
            .Select(_ =>
            {
                var bytes = new byte[4];
                RandomNumberGenerator.Fill(bytes);
                return Uuid.FromCounterV7(ref counter, ts, bytes);
            })
            .ToList();

        for (var i = 0; i < uuids.Count - 1; i++)
        {
            var a = uuids[i];
            var b = uuids[i + 1];

            Assert.Equal(Uuid.UuidVersion.V7, a.GetVersion());

            Assert.True(a < b, $"UUIDs are not ordered at {i}: {a} !< {b}");
            Assert.True(
                a.GetCounter() < b.GetCounter(),
                $"UUID counters are not ordered at {i}: {a.GetCounter()} !< {b.GetCounter()}"
            );
        }
    }

    [Fact]
    public static void UuidVersion()
    {
        var u = Uuid.NIL;
        Assert.Equal(Uuid.UuidVersion.Nil, u.GetVersion());

        u = Uuid.MAX;
        Assert.Equal(Uuid.UuidVersion.Max, u.GetVersion());

        var randomBytes = new byte[16];
        RandomNumberGenerator.Fill(randomBytes);
        u = Uuid.FromRandomBytesV4(randomBytes);
        Assert.Equal(Uuid.UuidVersion.V4, u.GetVersion());

        var counter = 0;
        u = Uuid.FromCounterV7(ref counter, Timestamp.UNIX_EPOCH, randomBytes.AsSpan()[..4]);
        Assert.Equal(Uuid.UuidVersion.V7, u.GetVersion());
    }
}
