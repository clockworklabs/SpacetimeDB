namespace SpacetimeDB.Testing.Tests;

using SpacetimeDB.Internal;
using TestingModule;

public class ProcedureTests
{
    [Fact]
    public void TransactionsCommitIndependentlyAndRollbackFailures()
    {
        using var test = TestContext.Create(typeof(Entry).Assembly);
        var procedure = test.ProcedureContext(TestAuth.Internal());
        var committed = procedure.WithTx(tx =>
            tx.Db.Entries.Insert(new Entry { Text = "committed" })
        );
        var failure = new InvalidOperationException("failed transaction");
        Assert.Same(
            failure,
            Assert.Throws<InvalidOperationException>(
                () =>
                    procedure.WithTx<int>(tx =>
                    {
                        tx.Db.Entries.Insert(new Entry { Text = "rolled back" });
                        throw failure;
                    })
            )
        );
        var rejected = procedure.TryWithTx(tx =>
        {
            tx.Db.Entries.Insert(new Entry { Text = "returned error" });
            return Result<int, InvalidOperationException>.Err(failure);
        });
        Assert.False(rejected.IsSuccess);
        Assert.Same(failure, rejected.Error);
        var success = procedure.TryWithTx(tx =>
            Result<ulong, InvalidOperationException>.Ok(tx.Db.Entries.Count)
        );
        Assert.True(success.IsSuccess);
        Assert.Equal(1UL, success.Value);
        Assert.Equal(committed.Id, Assert.Single(test.Db.Entries.Iter()).Id);
        // WithTx treats Result.Err as an ordinary return value, unlike TryWithTx.
        procedure.WithTx(tx =>
        {
            tx.Db.Entries.Insert(new Entry { Text = "ordinary return" });
            return Result<int, InvalidOperationException>.Err(failure);
        });
        Assert.Equal(2UL, test.Db.Entries.Count);
    }

    [Fact]
    public void ContextsShareFixtureServicesButNeverReuseTransactionOwnership()
    {
        using var test = TestContext.Create(typeof(Entry).Assembly);
        var auth = TestAuth.FromJwtPayload(
            """{"iss":"test","sub":"client"}""",
            ConnectionId.FromHexString("00000000000000000000000000000001")!.Value
        );
        test.Clock.Set(new Timestamp(10));
        var procedure = test.ProcedureContext(auth);
        Assert.Equal(new Timestamp(10), procedure.Timestamp);
        Assert.Equal("client", procedure.SenderAuth.Jwt!.Subject);
        test.SetEnv("OPTIONAL", "before");
        Assert.Equal("before", procedure.Env.OPTIONAL);
        test.Clock.Set(new Timestamp(20));
        ProcedureTxContext? escaped = null;
        ProcedureTxContext? selected = null;
        procedure.WithTx(tx =>
        {
            escaped = tx;
            selected = ((IModuleContext<ProcedureTxContext>)tx).SelectInstance(1);
            Assert.Equal(1, ((IModuleContext)selected).InstanceId);
            Assert.Same(tx.Db.Access, selected.Db.Access);
            Assert.Equal(procedure.Sender, tx.Sender);
            Assert.Equal(procedure.ConnectionId, tx.ConnectionId);
            Assert.Same(procedure.SenderAuth, tx.SenderAuth);
            Assert.Same(procedure.Rng, tx.Rng);
            Assert.Equal(new Timestamp(20), tx.Timestamp);
            test.Clock.Set(new Timestamp(30));
            Assert.Equal(new Timestamp(20), tx.Timestamp);
            test.SetEnv("OPTIONAL", "during");
            Assert.Equal("during", tx.Env.OPTIONAL);
            Assert.Equal("during", selected.Env.OPTIONAL);
            Assert.Throws<InvalidOperationException>(
                () => procedure.WithTx(inner => inner.Db.Entries.Count)
            );
            return tx.Db.Entries.Insert(new Entry { Text = "still active" });
        });
        Assert.Equal(new Timestamp(20), procedure.Timestamp);
        var oldEnv = escaped!.Env;
        procedure.WithTx(tx =>
        {
            Assert.NotSame(escaped, tx);
            Assert.Equal(new Timestamp(30), tx.Timestamp);
            Assert.Throws<ObjectDisposedException>(() => oldEnv.OPTIONAL);
            Assert.Throws<ObjectDisposedException>(() => selected!.Db.Access!.CheckActive());
            Assert.Throws<ObjectDisposedException>(() => escaped.Db.Entries.Count);
            return 0;
        });
        Assert.Throws<InvalidOperationException>(() => procedure.Db.Entries.Count);
        test.Dispose();
        Assert.Throws<ObjectDisposedException>(() => procedure.WithTx(_ => 0));
        Assert.Throws<ObjectDisposedException>(() => procedure.Env.OPTIONAL);
    }

    [Fact]
    public void AwaitableCallbacksAreRejectedBeforeCommit()
    {
        using var test = TestContext.Create(typeof(Entry).Assembly);
        var procedure = test.ProcedureContext(TestAuth.Internal());
        Assert.Throws<ArgumentException>(() =>
        {
            _ = procedure.WithTx(async tx =>
            {
                tx.Db.Entries.Insert(new Entry { Text = "must not run" });
                await Task.Yield();
                return 1;
            });
        });
        Assert.Throws<ArgumentException>(
            () =>
                procedure.WithTx<object>(tx =>
                {
                    tx.Db.Entries.Insert(new Entry { Text = "rolled back" });
                    return Task.CompletedTask;
                })
        );
        var result = procedure.TryWithTx<object, Exception>(tx =>
        {
            tx.Db.Entries.Insert(new Entry { Text = "also rolled back" });
            return Result<object, Exception>.Ok(Task.CompletedTask);
        });
        Assert.IsType<ArgumentException>(result.Error);
        Assert.Equal(0UL, test.Db.Entries.Count);
    }
}
