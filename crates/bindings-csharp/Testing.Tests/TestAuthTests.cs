namespace SpacetimeDB.Testing.Tests;

using System.Text.Json;
using ChatModule;

public class TestAuthTests
{
    private static readonly ConnectionId connection = ConnectionId
        .FromHexString("00000000000000000000000000000001")!
        .Value;

    [Fact]
    public void ClientClaimsReachReducersAndAuthCanBeReusedAcrossFixtures()
    {
        const string payload =
            """{"iss":"test-issuer","sub":"alice","aud":["app","other"],"role":"admin"}""";
        var auth = TestAuth.FromJwtPayload(payload, connection);
        using var first = TestContext.Create(typeof(Module).Assembly);
        using var second = TestContext.Create(typeof(Module).Assembly);

        void Send(TestContext test) =>
            test.WithReducerTx(
                auth,
                ctx =>
                {
                    Assert.False(ctx.SenderAuth.IsInternal);
                    Assert.True(ctx.SenderAuth.HasJwt);
                    Assert.Equal(connection, ctx.ConnectionId);
                    var jwt = Assert.IsType<JwtClaims>(ctx.SenderAuth.Jwt);
                    Assert.Equal(ctx.Sender, jwt.Identity);
                    Assert.Equal("test-issuer", jwt.Issuer);
                    Assert.Equal("alice", jwt.Subject);
                    Assert.Equal(["app", "other"], jwt.Audience);
                    Assert.Equal(payload, jwt.RawPayload);
                    Module.SendMessage(ctx, "Hello!");
                }
            );
        Parallel.Invoke(() => Send(first), () => Send(second));
        var sender = Assert.Single(first.Db.Message.Iter()).Sender;
        Assert.NotEqual(first.DatabaseIdentity, sender);
        Assert.Equal(sender, Assert.Single(second.Db.Message.Iter()).Sender);

        var otherConnection = ConnectionId.FromHexString("00000000000000000000000000000002")!.Value;
        var samePrincipal = TestAuth.FromJwtPayload(
            """{"iss":"test-issuer","sub":"alice","aud":"app"}""",
            otherConnection
        );
        first.WithReducerTx(
            samePrincipal,
            ctx =>
            {
                Assert.Equal(sender, ctx.Sender);
                Assert.Equal(otherConnection, ctx.ConnectionId);
                Assert.Equal(["app"], ctx.SenderAuth.Jwt!.Audience);
            }
        );
        var bob = TestAuth.FromJwtPayload("""{"iss":"test-issuer","sub":"bob"}""", connection);
        first.WithReducerTx(bob, ctx => Assert.NotEqual(sender, ctx.Sender));
    }

    [Fact]
    public void InvalidClaimsAndZeroConnectionIdsAreRejected()
    {
        Assert.Throws<ArgumentNullException>(() => TestAuth.FromJwtPayload(null!, connection));
        Assert.Throws<ArgumentException>(() =>
            TestAuth.FromJwtPayload("""{"iss":"issuer","sub":"subject"}""", default)
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
            foreach (
                var value in new object?[]
                {
                    null,
                    "",
                    new string('\u00e9', 65),
                }
            )
            {
                var claims = new Dictionary<string, object?>
                {
                    ["iss"] = "issuer",
                    ["sub"] = "subject",
                    [name] = value,
                };
                Assert.Throws<ArgumentException>(() =>
                    TestAuth.FromJwtPayload(JsonSerializer.Serialize(claims), connection)
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
            Assert.Throws<ArgumentException>(() =>
                TestAuth.FromJwtPayload(JsonSerializer.Serialize(claims), connection)
            );
        }
    }
}
