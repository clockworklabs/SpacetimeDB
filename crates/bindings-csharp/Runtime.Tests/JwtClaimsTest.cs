namespace Runtime.Tests;
using SpacetimeDB;

public class JwtClaimsTest
{
    [Fact]
    public void Test1()
    {
        var jwt = new JwtClaims("{\"sub\":\"1234567890\",\"name\":\"John Doe\",\"iat\":1516239022}");
        Assert.Equal("1234567890", jwt.Subject);
    }
}