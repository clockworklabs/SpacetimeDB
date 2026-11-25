namespace SpacetimeDB.Testing.Tests;

using TestingModule;

public class ProcedureTests
{
    [Fact]
    public void TransactionsCommitIndependentlyAndRollbackFailures()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var procedure = test.CreateProcedureContext(TestAuth.Internal());
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
    public void TransactionsShareTheProcedureCallerAndTakeTheClockAtEntry()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var client = TestAuth.FromJwtPayload(
            """{"iss":"test","sub":"client"}""",
            ConnectionId.FromHexString("00000000000000000000000000000001")!.Value
        );
        test.Clock.Set(new Timestamp(10));
        var procedure = test.CreateProcedureContext(client);
        Assert.Equal(new Timestamp(10), procedure.Timestamp);
        Assert.Equal("client", procedure.SenderAuth.Jwt!.Subject);

        test.Clock.Set(new Timestamp(20));
        procedure.WithTx(tx =>
        {
            Assert.Equal(procedure.Sender, tx.Sender);
            Assert.Equal(procedure.ConnectionId, tx.ConnectionId);
            Assert.Same(procedure.Rng, tx.Rng);
            test.Clock.Set(new Timestamp(30));
            test.SetEnv("OPTIONAL", "during");
            Assert.Equal(new Timestamp(20), tx.Timestamp);
            Assert.Equal("during", tx.Env.OPTIONAL);
            Assert.Throws<InvalidOperationException>(() => procedure.WithTx(_ => 0));
            return 0;
        });
        procedure.WithTx(tx =>
        {
            Assert.Equal(new Timestamp(30), tx.Timestamp);
            return 0;
        });
    }

    [Fact]
    public void TransactionHandlesExpireWithTheirTransaction()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var procedure = test.CreateProcedureContext(TestAuth.Internal());
        Assert.Throws<InvalidOperationException>(() => procedure.Db.Entries.Count);
        var escaped = procedure.WithTx(tx => tx);
        Assert.Throws<ObjectDisposedException>(() => escaped.Db.Entries.Count);
        Assert.Throws<ObjectDisposedException>(() => escaped.Env.OPTIONAL);
        test.Dispose();
        Assert.Throws<ObjectDisposedException>(() => procedure.WithTx(_ => 0));
    }

    [Fact]
    public void AwaitableCallbacksAreRejectedBeforeCommit()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var procedure = test.CreateProcedureContext(TestAuth.Internal());
        Assert.Throws<ArgumentException>(() =>
        {
            _ = procedure.WithTx(async tx =>
            {
                tx.Db.Entries.Insert(new Entry { Text = "must not run" });
                await Task.Yield();
                return 1;
            });
        });
        // Invalid callbacks are rejected before the transaction starts, not reported as outcomes.
        Assert.Throws<ArgumentException>(
            () =>
                procedure.TryWithTx<Task, Exception>(tx =>
                {
                    tx.Db.Entries.Insert(new Entry { Text = "must not run" });
                    return Result<Task, Exception>.Ok(Task.CompletedTask);
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
