using SpacetimeDB;
using SpacetimeDB.Testing;
using Xunit;

public class ModuleTests
{
    [Fact]
    public void ConnectedUserCanSendMessage()
    {
        using var test = ModuleTestContext.Create(typeof(Module).Assembly);
        var auth = TestAuth.FromJwtPayload(
            """{"iss":"test-issuer","sub":"alice"}""",
            ConnectionId.FromHexString("00000000000000000000000000000001")!.Value
        );
        var now = new Timestamp(1_000_000);
        test.Clock.Set(now);

        test.WithReducerTx(auth, Module.ClientConnected);
        test.WithReducerTx(auth, ctx => Module.SetName(ctx, "Alice"));
        test.WithReducerTx(auth, ctx => Module.SendMessage(ctx, "Hello!"));

        var user = Assert.Single(test.Db.User.Iter());
        Assert.Equal("Alice", user.Name);
        Assert.True(test.Db.User.Identity.Find(user.Identity)!.Online);
        var message = Assert.Single(test.Db.Message.Iter());
        Assert.Equal(user.Identity, message.Sender);
        Assert.Equal(now, message.Sent);
        Assert.Equal("Hello!", message.Text);
    }

    [Fact]
    public void FailedTransactionRollsBackEarlierWrites()
    {
        using var test = ModuleTestContext.Create(typeof(Module).Assembly);
        var auth = TestAuth.Internal();

        Assert.Throws<ArgumentException>(() => test.WithReducerTx(auth, ctx =>
        {
            Module.SendMessage(ctx, "This write is rolled back");
            Module.SendMessage(ctx, "");
        }));

        Assert.Empty(test.Db.Message.Iter());
    }
}
