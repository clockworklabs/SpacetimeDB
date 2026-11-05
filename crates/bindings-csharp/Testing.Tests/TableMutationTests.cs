namespace SpacetimeDB.Testing.Tests;

using TestingModule;

public class TableMutationTests
{
    [Fact]
    public void DeleteAndClearUseTheOwningTransaction()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var auth = TestAuth.Internal();
        var first = test.Db.Entries.Insert(new Entry { Text = "first" });
        var second = test.Db.Entries.Insert(new Entry { Text = "second" });
        Assert.False(test.Db.Entries.Delete(new Entry { Id = first.Id, Text = "different" }));

        ReducerContext? escaped = null;
        Assert.Throws<InvalidOperationException>(
            () =>
                test.WithReducerTx(
                    auth,
                    ctx =>
                    {
                        escaped = ctx;
                        Assert.True(ctx.Db.Entries.Delete(first));
                        Assert.False(ctx.Db.Entries.Delete(first));
                        Assert.Equal(1UL, ctx.Db.Entries.Clear());
                        Assert.Equal(0UL, ctx.Db.Entries.Clear());
                        Assert.Equal(0UL, ctx.Db.Entries.Count);
                        throw new InvalidOperationException("rollback");
                    }
                )
        );
        Assert.Equal(2UL, test.Db.Entries.Count);
        Assert.Throws<ObjectDisposedException>(() => escaped!.Db.Entries.Delete(first));
        Assert.Throws<ObjectDisposedException>(() => escaped!.Db.Entries.Clear());

        test.WithReducerTx(auth, ctx => Assert.True(ctx.Db.Entries.Delete(first)));
        Assert.Equal(second.Id, Assert.Single(test.Db.Entries.Iter()).Id);
        Assert.True(test.Db.Entries.Delete(second));
        Assert.False(test.Db.Entries.Delete(second));

        var third = test.Db.Entries.Insert(new Entry { Text = "third" });
        test.WithReducerTx(auth, ctx => Assert.Equal(1UL, ctx.Db.Entries.Clear()));
        Assert.Equal(0UL, test.Db.Entries.Count);
        var fourth = test.Db.Entries.Insert(new Entry { Text = "fourth" });
        Assert.True(fourth.Id > third.Id);
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
