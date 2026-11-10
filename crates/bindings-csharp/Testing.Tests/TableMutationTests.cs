namespace SpacetimeDB.Testing.Tests;

using TestingModule;

public class TableMutationTests
{
    [Fact]
    public void UniqueViolationInAReducerRollsBackEarlierWrites()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        Assert.Throws<UniqueConstraintViolationException>(
            () =>
                test.WithReducerTx(
                    TestAuth.Internal(),
                    ctx =>
                    {
                        ctx.Db.Entries.Insert(new Entry { Text = "earlier write" });
                        ctx.Db.Accounts.Insert(new Account { Id = 1, Name = "taken" });
                        ctx.Db.Accounts.Insert(new Account { Id = 2, Name = "taken" });
                    }
                )
        );
        Assert.Equal(0UL, test.Db.Entries.Count);
        Assert.Equal(0UL, test.Db.Accounts.Count);
    }

    [Fact]
    public void RolledBackSequenceValuesAreReused()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var committed = test.Db.Entries.Insert(new Entry { Text = "committed" }).Id;
        ulong rolledBack = 0;
        Assert.Throws<InvalidOperationException>(
            () =>
                test.WithReducerTx(
                    TestAuth.Internal(),
                    ctx =>
                    {
                        rolledBack = ctx.Db.Entries.Insert(new Entry { Text = "discarded" }).Id;
                        throw new InvalidOperationException("rollback");
                    }
                )
        );
        Assert.True(rolledBack > committed);
        // The sequence's allocation is part of the transaction, so rollback returns it.
        Assert.Equal(rolledBack, test.Db.Entries.Insert(new Entry { Text = "next" }).Id);
    }

    [Fact]
    public void DeleteAndClearUseTheOwningTransaction()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var auth = TestAuth.Internal();
        var first = test.Db.Entries.Insert(new Entry { Text = "first" });
        var second = test.Db.Entries.Insert(new Entry { Text = "second" });
        // Delete matches the whole row, not just its key.
        Assert.False(test.Db.Entries.Delete(new Entry { Id = first.Id, Text = "different" }));

        Assert.Throws<InvalidOperationException>(
            () =>
                test.WithReducerTx(
                    auth,
                    ctx =>
                    {
                        Assert.True(ctx.Db.Entries.Delete(first));
                        Assert.False(ctx.Db.Entries.Delete(first));
                        Assert.Equal(1UL, ctx.Db.Entries.Clear());
                        throw new InvalidOperationException("rollback");
                    }
                )
        );
        Assert.Equal(2UL, test.Db.Entries.Count);

        test.WithReducerTx(auth, ctx => Assert.True(ctx.Db.Entries.Delete(first)));
        Assert.Equal(second.Id, Assert.Single(test.Db.Entries.Iter()).Id);
        Assert.Equal(1UL, test.Db.Entries.Clear());
        Assert.Equal(0UL, test.Db.Entries.Clear());
    }

    [Fact]
    public void EventRowsExistOnlyInsideTheirTransaction()
    {
        using var test = ModuleTestContext.Create(typeof(TestEvent).Assembly);
        var auth = TestAuth.Internal();
        test.WithReducerTx(
            auth,
            ctx =>
            {
                ctx.Db.Events.Insert(new TestEvent { Text = "committed" });
                Assert.Equal("committed", Assert.Single(ctx.Db.Events.Iter()).Text);
            }
        );
        Assert.Equal(0UL, test.Db.Events.Count);

        Assert.Throws<InvalidOperationException>(
            () =>
                test.WithReducerTx(
                    auth,
                    ctx =>
                    {
                        ctx.Db.Events.Insert(new TestEvent { Text = "rolled back" });
                        Assert.Equal(1UL, ctx.Db.Events.Count);
                        throw new InvalidOperationException("rollback");
                    }
                )
        );
        Assert.Equal(0UL, test.Db.Events.Count);

        test.Db.Events.Insert(new TestEvent { Text = "standalone" });
        Assert.Empty(test.Db.Events.Iter());
    }
}
