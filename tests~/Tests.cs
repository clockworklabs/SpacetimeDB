using SpacetimeDB;
using SpacetimeDB.BSATN;
using SpacetimeDB.Types;

public class Tests
{
    [Fact]
    public static void GenericEqualityComparerCheck()
    {
        // Validates the behavior of the GenericEqualityComparer's Equals function

        // Byte Arrays
        byte[] byteArray = new byte[10];
        byte[] byteArrayByRef = byteArray;
        byte[] byteArrayByValue = new byte[10];
        byte[] byteArrayUnequalValue = new byte[01];

        Assert.True(GenericEqualityComparer.Instance.Equals(byteArray, byteArrayByRef));
        Assert.True(GenericEqualityComparer.Instance.Equals(byteArray, byteArrayByValue));
        Assert.False(GenericEqualityComparer.Instance.Equals(byteArray, byteArrayUnequalValue));

        // Integers
        int integer = 5;
        int integerByValue = 5;
        int integerUnequalValue = 7;
        string integerAsDifferingType = "5";

        Assert.True(GenericEqualityComparer.Instance.Equals(integer, integerByValue));
        Assert.False(GenericEqualityComparer.Instance.Equals(integer, integerUnequalValue));
        // GenericEqualityComparer does not support to converting datatypes and will fail this test
        Assert.False(GenericEqualityComparer.Instance.Equals(integer, integerAsDifferingType));

        // String
        string testString = "This is a test";
        string testStringByRef = testString;
        string testStringByValue = "This is a test";
        string testStringUnequalValue = "This is not the same string";

        Assert.True(GenericEqualityComparer.Instance.Equals(testString, testStringByRef));
        Assert.True(GenericEqualityComparer.Instance.Equals(testString, testStringByValue));
        Assert.False(GenericEqualityComparer.Instance.Equals(testString, testStringUnequalValue));

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

        Assert.True(GenericEqualityComparer.Instance.Equals(identity, identityByRef));
        Assert.True(GenericEqualityComparer.Instance.Equals(identity, identityByValue));
        Assert.False(GenericEqualityComparer.Instance.Equals(identity, identityUnequalValue));

        Assert.True(GenericEqualityComparer.Instance.Equals(testUser, testUserByRef));
        Assert.True(GenericEqualityComparer.Instance.Equals(testUser, testUserByValue));
        Assert.False(GenericEqualityComparer.Instance.Equals(testUser, testUserUnequalIdentityValue));
        Assert.False(GenericEqualityComparer.Instance.Equals(testUser, testUserUnequalNameValue));
        Assert.False(GenericEqualityComparer.Instance.Equals(testUser, testUserUnequalOnlineValue));

        // TaggedEnum using Status record
        Status statusCommitted = new Status.Committed(default);
        Status statusCommittedByRef = statusCommitted;
        Status statusCommittedByValue = new Status.Committed(default);
        Status statusFailed = new Status.Failed("Failed");
        Status statusFailedByValue = new Status.Failed("Failed");
        Status statusFailedUnequalValue = new Status.Failed("unequalFailed");
        Status statusOutOfEnergy = new Status.OutOfEnergy(default);

        Assert.True(GenericEqualityComparer.Instance.Equals(statusCommitted, statusCommittedByRef));
        Assert.True(GenericEqualityComparer.Instance.Equals(statusCommitted, statusCommittedByValue));
        Assert.False(GenericEqualityComparer.Instance.Equals(statusCommitted, statusFailed));
        Assert.True(GenericEqualityComparer.Instance.Equals(statusFailed, statusFailedByValue));
        Assert.False(GenericEqualityComparer.Instance.Equals(statusFailed, statusFailedUnequalValue));
        Assert.False(GenericEqualityComparer.Instance.Equals(statusCommitted, statusOutOfEnergy));
    }
}