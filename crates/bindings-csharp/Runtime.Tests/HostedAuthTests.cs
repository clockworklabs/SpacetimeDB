namespace Runtime.Tests;

using SpacetimeDB;

public class HostedAuthTests
{
    [Theory]
    [InlineData(0u, false)]
    [InlineData(1u, true)]
    public void NoJwtCallsPreserveVerifiedInternalFlag(uint flags, bool expectedInternal)
    {
        var auth = AuthCtx.FromVerifiedCall(flags, () => null);
        Assert.Equal(expectedInternal, auth.IsInternal);
        Assert.False(auth.HasJwt);
        Assert.Null(auth.Jwt);
    }

    [Fact]
    public void InternalCallCanRetainJwtAndVerifiedSenderIdentity()
    {
        var sender = Identity.FromHexString(new string('a', 64));
        var reads = 0;
        var flags = 1u;
        var auth = AuthCtx.FromVerifiedCall(
            flags,
            () =>
            {
                reads++;
                return new JwtClaims(
                    "{\"iss\":\"different-issuer\",\"sub\":\"different-subject\",\"identity\":\"untrusted\"}",
                    sender
                );
            }
        );
        flags = 0;
        Assert.True(auth.IsInternal);
        Assert.Equal(0, reads);
        Assert.True(auth.HasJwt);
        Assert.Equal(sender, auth.Jwt!.Identity);
        Assert.Equal("different-subject", auth.Jwt.Subject);
        Assert.Equal(1, reads);
    }
}
