namespace Runtime.Tests;

using SpacetimeDB;

public class HandlerContextTests
{
    private sealed class TestLocal : LocalBase { }

    private sealed class TestTxContext(SpacetimeDB.Internal.TxContext inner)
        : HandlerTxContextBase(inner) { }

    private sealed class TestHandlerContext() : HandlerContextBase(new Random(0), new Timestamp(0))
    {
        protected override LocalBase CreateLocal() => new TestLocal();

        protected override HandlerTxContextBase CreateTxContext(SpacetimeDB.Internal.TxContext inner) =>
            new TestTxContext(inner);
    }

    [Fact]
    public void HandlerTransactionsAreExternalWithoutJwt()
    {
        var handler = new TestHandlerContext();
        // EnterTxContext is the construction path used by WithTx and TryWithTx,
        // including when a failed commit is retried with a new timestamp.
        for (long timestamp = 0; timestamp < 3; timestamp++)
        {
            var auth = handler.EnterTxContext(timestamp).SenderAuth;
            Assert.False(auth.IsInternal);
            Assert.False(auth.HasJwt);
            Assert.Null(auth.Jwt);
        }
        handler.ExitTxContext();
        Assert.False(handler.EnterTxContext(3).SenderAuth.IsInternal);
    }
}
