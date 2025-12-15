using System.Diagnostics;
using CsCheck;
using SpacetimeDB;
using SpacetimeDB.BSATN;
using SpacetimeDB.Types;

public class Tests
{
    [Fact]
    public static void DefaultEqualityComparerCheck()
    {
        // Sanity check on the behavior of the default EqualityComparer's Equals function w.r.t. spacetime types.
        var comparer = EqualityComparer<object>.Default;

        // Integers
        int integer = 5;
        int integerByValue = 5;
        int integerUnequalValue = 7;
        string integerAsDifferingType = "5";

        Assert.True(comparer.Equals(integer, integerByValue));
        Assert.False(comparer.Equals(integer, integerUnequalValue));
        // GenericEqualityComparer does not support to converting datatypes and will fail this test
        Assert.False(comparer.Equals(integer, integerAsDifferingType));

        // String
        string testString = "This is a test";
        string testStringByRef = testString;
        string testStringByValue = "This is a test";
        string testStringUnequalValue = "This is not the same string";

        Assert.True(comparer.Equals(testString, testStringByRef));
        Assert.True(comparer.Equals(testString, testStringByValue));
        Assert.False(comparer.Equals(testString, testStringUnequalValue));

        // Note: We are limited to only [SpacetimeDB.Type]

        // Identity and User
        Identity identity = Identity.From(Convert.FromBase64String("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY="));
        Identity identityByRef = identity;
        Identity identityByValue = Identity.From(Convert.FromBase64String("l0qzG1GPRtC1mwr+54q98tv0325gozLc6cNzq4vrzqY="));
        Identity identityUnequalValue = Identity.From(Convert.FromBase64String("j5DMlKmWjfbSl7qmZQOok7HDSwsAJopRSJjdlUsNogs="));

        User testUser = new User { Identity = identity, Name = "name", Online = false };
        User testUserByRef = testUser;
        User testUserByValue = new User { Identity = identity, Name = "name", Online = false };
        User testUserUnequalIdentityValue = new User { Identity = identityUnequalValue, Name = "name", Online = false };
        User testUserUnequalNameValue = new User { Identity = identity, Name = "unequalName", Online = false };
        User testUserUnequalOnlineValue = new User { Identity = identity, Name = "name", Online = true };

        Assert.True(comparer.Equals(identity, identityByRef));
        Assert.True(comparer.Equals(identity, identityByValue));
        Assert.False(comparer.Equals(identity, identityUnequalValue));

        Assert.True(comparer.Equals(testUser, testUserByRef));
        Assert.True(comparer.Equals(testUser, testUserByValue));
        Assert.False(comparer.Equals(testUser, testUserUnequalIdentityValue));
        Assert.False(comparer.Equals(testUser, testUserUnequalNameValue));
        Assert.False(comparer.Equals(testUser, testUserUnequalOnlineValue));

        // TaggedEnum using Status record
        Status statusCommitted = new Status.Committed(default);
        Status statusCommittedByRef = statusCommitted;
        Status statusCommittedByValue = new Status.Committed(default);
        Status statusFailed = new Status.Failed("Failed");
        Status statusFailedByValue = new Status.Failed("Failed");
        Status statusFailedUnequalValue = new Status.Failed("unequalFailed");
        Status statusOutOfEnergy = new Status.OutOfEnergy(default);

        Assert.True(comparer.Equals(statusCommitted, statusCommittedByRef));
        Assert.True(comparer.Equals(statusCommitted, statusCommittedByValue));
        Assert.False(comparer.Equals(statusCommitted, statusFailed));
        Assert.True(comparer.Equals(statusFailed, statusFailedByValue));
        Assert.False(comparer.Equals(statusFailed, statusFailedUnequalValue));
        Assert.False(comparer.Equals(statusCommitted, statusOutOfEnergy));
    }

    [Fact]
    public static void ListstreamWorks()
    {
        // Make sure ListStream behaves like MemoryStream.

        int listLength = 32;
        Gen.Select(Gen.Byte.List[listLength], Gen.Int[0, 10].SelectMany(n => Gen.Int[0, listLength + 5].List[n].Select(list =>
        {
            list.Sort();
            return list;
        })), (list, cuts) => (list, cuts)).Sample((listCuts) =>
        {
            var (list, cuts) = listCuts;
            var listStream = new ListStream(list);
            var memoryStream = new MemoryStream(list.ToArray());

            for (var i = 0; i < cuts.Count - 1; i++)
            {
                var start = cuts[i];
                var end = cuts[i + 1];

                var arr1 = new byte[end - start];
                Span<byte> span1 = arr1;

                var arr2 = new byte[end - start];
                Span<byte> span2 = arr2;

                var readList = listStream.Read(span1);
                var readMemory = memoryStream.Read(span2);
                Debug.Assert(readList == readMemory);
                Debug.Assert(span1.SequenceEqual(span2));
            }

            listStream = new ListStream(list);
            memoryStream = new MemoryStream(list.ToArray());

            for (var i = 0; i < cuts.Count - 1; i++)
            {
                var start = cuts[i];
                var end = cuts[i + 1];
                var len = end - start;

                var arr1 = new byte[len + 3];
                var arr2 = new byte[len + 3];

                // this is a janky way to choose the offset but I don't feel like plumbing in another randomized list
                var readList = listStream.Read(arr1, len % 3, len);
                var readMemory = memoryStream.Read(arr2, len % 3, len);
                Debug.Assert(readList == readMemory);
                Debug.Assert(arr1.SequenceEqual(arr2));
            }
        });
    }

    public class BTreeIndexBaseColumnImplementsIComparableTest
    {

        public sealed class UserHandle : RemoteTableHandle<EventContext, User>
        {
            protected override string RemoteTableName => "user";

            public sealed class IdentityIndex : BTreeIndexBase<SpacetimeDB.Identity>
            {
                protected override SpacetimeDB.Identity GetKey(User row) => row.Identity;

                public IdentityIndex(UserHandle table) : base(table) { }
            }

            public readonly IdentityIndex Identity;

            internal UserHandle(DbConnection conn) : base(conn)
            {
                Identity = new(this);
            }
        }

        [Fact]
        public void Identity_ShouldImplementIComparable()
        {
            // Arrange
            var identityType = typeof(SpacetimeDB.Identity);

            // Act
            bool implementsIComparable =
                typeof(IComparable<>).MakeGenericType(identityType).IsAssignableFrom(identityType);

            // Assert
            Assert.True(implementsIComparable, $"{identityType} does not implement IComparable<{identityType}>");
        }

        [Fact]
        public void IdentityIndex_ShouldInheritFrom_BTreeIndexBase()
        {
            // Arrange
            var identityIndexType = typeof(UserHandle.IdentityIndex);
            var expectedBaseType = typeof(RemoteTableHandle<EventContext, User>.BTreeIndexBase<SpacetimeDB.Identity>);

            // Act
            bool isCorrectBaseType = expectedBaseType.IsAssignableFrom(identityIndexType.BaseType);

            // Assert
            Assert.True(isCorrectBaseType,
                "IdentityIndex does not correctly inherit from BTreeIndexBase<SpacetimeDB.Identity>");
        }
    }
}