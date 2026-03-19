namespace Runtime.Tests;

using SpacetimeDB;

public class JwtClaimsTest
{
    [Fact]
    public void TestSubject()
    {
        var jwt = new JwtClaims(
            "{\"sub\":\"123\",\"name\":\"John Doe\",\"iss\":\"example.com\"}",
            Identity.FromHexString(
                "c200ef884364c1a99be0298dc68f2004e6b97c09d1b1658b7db22f51fb662059"
            )
        );
        Assert.Equal("123", jwt.Subject);
    }
}
