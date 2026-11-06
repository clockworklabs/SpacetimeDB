namespace SpacetimeDB.Testing.Tests;

using TestingModule;

public class ProcedureHookTests
{
    [Fact]
    public void HooksRunAfterClosureAndOnlyForCommittedTransactions()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        ProcedureTxContext? escaped = null;
        var calls = 0;
        var builder = test.CreateProcedureContextBuilder(TestAuth.Internal())
            .AfterTxCommit(fixture =>
            {
                Assert.Same(test, fixture);
                Assert.Throws<ObjectDisposedException>(() => escaped!.Db.Entries.Count);
                Assert.Equal((ulong)(calls + 1), fixture.Db.Entries.Count);
                fixture.Db.Entries.Insert(new Entry { Text = "hook" });
                fixture.Clock.Advance(new TimeDuration(10));
                calls++;
            });
        var procedure = builder.Build();
        builder.AfterTxCommit(_ =>
            throw new InvalidOperationException("must not replace built hook")
        );
        procedure.WithTx(tx =>
        {
            escaped = tx;
            return tx.Db.Entries.Insert(new Entry { Text = "procedure" });
        });
        var failure = new InvalidOperationException("rollback");
        Assert.Throws<InvalidOperationException>(() => procedure.WithTx<int>(_ => throw failure));
        var rejected = procedure.TryWithTx(_ => Result<int, Exception>.Err(failure));
        Assert.Same(failure, rejected.Error);
        Assert.Equal(1, calls);
        var committed = procedure.TryWithTx(tx =>
        {
            escaped = tx;
            Assert.Equal(new Timestamp(10), tx.Timestamp);
            return Result<int, Exception>.Ok(0);
        });
        Assert.True(committed.IsSuccess);
        Assert.Equal(2, calls);
        Assert.Equal(3UL, test.Db.Entries.Count);
        Assert.Equal(new Timestamp(20), test.Clock.Now);
    }

    [Fact]
    public void HookFailuresPropagateWithoutUndoingCommits()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var calls = 0;
        var failure = new InvalidOperationException("hook failure");
        var procedure = test.CreateProcedureContextBuilder(TestAuth.Internal())
            .AfterTxCommit(_ =>
            {
                calls++;
                throw failure;
            })
            .Build();
        // A selected context shares the procedure's hook and runs it once per commit.
        var selected = ((Internal.IModuleContext<ProcedureContext>)procedure).SelectInstance(1);
        Assert.Same(
            failure,
            Assert.Throws<InvalidOperationException>(
                () => selected.WithTx(tx => tx.Db.Entries.Insert(new Entry { Text = "WithTx" }))
            )
        );
        Assert.Equal(1, calls);
        Assert.Same(
            failure,
            Assert.Throws<InvalidOperationException>(
                () =>
                    procedure.TryWithTx(tx =>
                        Result<Entry, Exception>.Ok(
                            tx.Db.Entries.Insert(new Entry { Text = "TryWithTx" })
                        )
                    )
            )
        );
        Assert.Equal(
            ["TryWithTx", "WithTx"],
            test.Db.Entries.Iter().Select(row => row.Text).Order()
        );
    }
}
