namespace SpacetimeDB.Testing.Tests;

using TestingModule;

public class OutboxTests
{
    [Fact]
    public void OutboxRowsCanBeInsertedAndFound()
    {
        using var test = ModuleTestContext.Create(typeof(OutboxModule).Assembly);

        test.WithReducerTx(
            TestAuth.Internal(),
            ctx =>
            {
                var row = ctx.Db.PingOutbox.Insert(new PingOutboxRow { Body = "hello" });
                Assert.Equal("hello", ctx.Db.PingOutbox.Id.Find(row.Id)?.Body);
            }
        );
    }
}
