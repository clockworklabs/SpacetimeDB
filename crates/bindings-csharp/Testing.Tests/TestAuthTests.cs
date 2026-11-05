namespace SpacetimeDB.Testing.Tests;

using System.Text.Json;
using ChatModule;

public class TestAuthTests
{
    private static readonly ConnectionId connection = ConnectionId
        .FromHexString("00000000000000000000000000000001")!
        .Value;

    [Fact]
    public void DerivedIdentityMatchesTheServer()
    {
        // Identity::from_claims("test-issuer", "alice") from crates/lib, as the server derives it.
        var expected = Identity.FromHexString(
            "c2003d6ba171341c20c7d0f753d3a1c845615c7883fd7f1d5b6337e627c483fe"
        );
        var auth = TestAuth.FromJwtPayload("""{"iss":"test-issuer","sub":"alice"}""", connection);
        Assert.Equal(expected, auth.Sender);
    }

    [Fact]
    public void CredentialsExposeTheCallerTheyActAs()
    {
        var auth = TestAuth.FromJwtPayload("""{"iss":"test-issuer","sub":"alice"}""", connection);
        using var test = ModuleTestContext.Create(typeof(Module).Assembly);
        test.WithReducerTx(
            auth,
            ctx =>
            {
                Assert.Equal(ctx.Sender, auth.Sender);
                Assert.Equal(ctx.ConnectionId, auth.ConnectionId);
            }
        );

        var internalAuth = TestAuth.Internal();
        Assert.Null(internalAuth.Sender);
        Assert.Null(internalAuth.ConnectionId);
        test.WithReducerTx(
            internalAuth,
            ctx =>
            {
                Assert.Equal(test.DatabaseIdentity, ctx.Sender);
                Assert.Null(ctx.ConnectionId);
                Assert.True(ctx.SenderAuth.IsInternal);
            }
        );
    }

    [Fact]
    public void ClientClaimsReachReducers()
    {
        const string payload =
            """{"iss":"test-issuer","sub":"alice","aud":["app","other"],"role":"admin"}""";
        using var test = ModuleTestContext.Create(typeof(Module).Assembly);
        var sender = test.WithReducerTx(
            TestAuth.FromJwtPayload(payload, connection),
            ctx =>
            {
                Assert.False(ctx.SenderAuth.IsInternal);
                var jwt = Assert.IsType<JwtClaims>(ctx.SenderAuth.Jwt);
                Assert.Equal(ctx.Sender, jwt.Identity);
                Assert.Equal("test-issuer", jwt.Issuer);
                Assert.Equal("alice", jwt.Subject);
                Assert.Equal(["app", "other"], jwt.Audience);
                Assert.Equal(payload, jwt.RawPayload);
                return ctx.Sender;
            }
        );

        // The identity depends only on issuer and subject, not the connection or audience.
        var otherConnection = ConnectionId.FromHexString("00000000000000000000000000000002")!.Value;
        var samePrincipal = TestAuth.FromJwtPayload(
            """{"iss":"test-issuer","sub":"alice","aud":"app"}""",
            otherConnection
        );
        test.WithReducerTx(
            samePrincipal,
            ctx =>
            {
                Assert.Equal(sender, ctx.Sender);
                Assert.Equal(otherConnection, ctx.ConnectionId);
            }
        );
    }

    [Fact]
    public void InvalidClaimsAndZeroConnectionIdsAreRejected()
    {
        Assert.Throws<ArgumentNullException>(() => TestAuth.FromJwtPayload(null!, connection));
        Assert.Throws<ArgumentException>(
            () => TestAuth.FromJwtPayload("""{"iss":"issuer","sub":"subject"}""", default)
        );
        foreach (
            var payload in new[]
            {
                "not JSON",
                "[]",
                "{}",
                "{\"iss\":\"issuer\"}",
                """{"iss":"\uD800","sub":"subject"}""",
            }
        )
            Assert.Throws<ArgumentException>(() => TestAuth.FromJwtPayload(payload, connection));

        foreach (var name in new[] { "iss", "sub" })
        {
            foreach (var value in new object?[] { null, "", new string('\u00e9', 65) })
            {
                var claims = new Dictionary<string, object?>
                {
                    ["iss"] = "issuer",
                    ["sub"] = "subject",
                    [name] = value,
                };
                Assert.Throws<ArgumentException>(
                    () => TestAuth.FromJwtPayload(JsonSerializer.Serialize(claims), connection)
                );
            }
            var boundary = new Dictionary<string, string>
            {
                ["iss"] = "issuer",
                ["sub"] = "subject",
                [name] = new string('\u00e9', 64),
            };
            _ = TestAuth.FromJwtPayload(JsonSerializer.Serialize(boundary), connection);
        }
    }

    [Fact]
    public void OptionalHexIdentityMustMatchDerivedIdentity()
    {
        var auth = TestAuth.FromJwtPayload("""{"iss":"issuer","sub":"subject"}""", connection);
        var sender = auth.Sender!.Value;
        var claims = new Dictionary<string, object?>
        {
            ["iss"] = "issuer",
            ["sub"] = "subject",
            ["hex_identity"] = sender.ToString(),
        };
        var matching = TestAuth.FromJwtPayload(JsonSerializer.Serialize(claims), connection);
        Assert.Equal(sender, matching.Sender);
        foreach (
            var invalid in new object?[]
            {
                null,
                "not hex",
                new string('z', 64),
                new string('0', 64),
            }
        )
        {
            claims["hex_identity"] = invalid;
            Assert.Throws<ArgumentException>(
                () => TestAuth.FromJwtPayload(JsonSerializer.Serialize(claims), connection)
            );
        }
    }
}
