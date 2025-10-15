namespace Runtime.Tests;
using SpacetimeDB;

public class JwtClaimsTest
{
    [Fact]
    public void TestSubject()
    {
        var jwt = new JwtClaims("{\"sub\":\"1234567890\",\"name\":\"John Doe\",\"iat\":1516239022}");
        Assert.Equal("1234567890", jwt.Subject);
    }

    [Fact]
    public void TestIdentity()
    {
        var jwt = new JwtClaims("{\"sub\":\"123\",\"name\":\"John Doe\",\"iss\":\"example.com\"}");
        var identity = jwt.Identity;
        Assert.Equal(identity, Identity.FromHexString("c200ef884364c1a99be0298dc68f2004e6b97c09d1b1658b7db22f51fb662059"));
    }
}