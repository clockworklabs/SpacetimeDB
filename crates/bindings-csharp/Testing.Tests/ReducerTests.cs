namespace SpacetimeDB.Testing.Tests;

using ChatModule;
using TestingModule;

public class ReducerTests
{
    [Fact]
    public void ReducerCommitsCallerTimestampAndMessageAndRollsBackOnFailure()
    {
        using var test = ModuleTestContext.Create(typeof(Module).Assembly);
        var auth = TestAuth.Internal();
        var now = new Timestamp(1_234_567);
        test.Clock.Set(now);

        test.WithReducerTx(auth, ctx => Module.SendMessage(ctx, "Hello!"));

        var message = Assert.Single(test.Db.Message.Iter());
        Assert.Equal(test.DatabaseIdentity, message.Sender);
        Assert.Equal(now, message.Sent);
        Assert.Equal("Hello!", message.Text);

        var error = Assert.Throws<ArgumentException>(
            () =>
                test.WithReducerTx(
                    auth,
                    ctx =>
                    {
                        Module.SendMessage(ctx, "This write will roll back");
                        Module.SendMessage(ctx, "");
                    }
                )
        );

        Assert.Equal("Messages must not be empty", error.Message);
        Assert.Equal("Hello!", Assert.Single(test.Db.Message.Iter()).Text);
    }

    [Fact]
    public void ParallelFixturesHaveIndependentMessagesAndIds()
    {
        using var first = ModuleTestContext.Create(typeof(Module).Assembly);
        using var second = ModuleTestContext.Create(typeof(Module).Assembly);
        var auth = TestAuth.Internal();

        void Send(ModuleTestContext test, string text) =>
            test.WithReducerTx(
                auth,
                ctx =>
                {
                    Module.SendMessage(ctx, text);
                    // The documented message table has no generated ID; use the existing test table.
                    ctx.Db.Entries.Insert(new Entry { Text = text });
                }
            );

        Parallel.Invoke(() => Send(first, "first"), () => Send(second, "second"));

        Assert.Equal("first", Assert.Single(first.Db.Message.Iter()).Text);
        Assert.Equal("second", Assert.Single(second.Db.Message.Iter()).Text);
        Assert.Equal(1UL, Assert.Single(first.Db.Entries.Iter()).Id);
        Assert.Equal(1UL, Assert.Single(second.Db.Entries.Iter()).Id);
    }
}
