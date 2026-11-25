namespace SpacetimeDB.Testing.Tests;

using TestingModule;

public class ModuleTestContextTests
{
    [Fact]
    public void ReducerTransactionsCommitRollbackAndInvalidateHandles()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var auth = TestAuth.Internal();
        ReducerContext? escaped = null;
        var inserted = test.WithReducerTx(
            auth,
            ctx =>
            {
                escaped = ctx;
                return ctx.Db.Entries.Insert(new Entry { Text = "committed" });
            }
        );
        Assert.Equal(inserted.Id, Assert.Single(test.Db.Entries.Iter()).Id);
        Assert.Throws<ObjectDisposedException>(() => escaped!.Db.Entries.Count);

        var failure = new InvalidOperationException("original exception");
        Assert.Same(
            failure,
            Assert.Throws<InvalidOperationException>(
                () =>
                    test.WithReducerTx(
                        auth,
                        ctx =>
                        {
                            ctx.Db.Entries.Insert(new Entry { Text = "rolled back" });
                            throw failure;
                        }
                    )
            )
        );
        Assert.Equal(1UL, test.Db.Entries.Count);

        // Ordinary error return values still commit.
        var result = test.WithReducerTx(
            auth,
            ctx =>
            {
                ctx.Db.Entries.Insert(new Entry { Text = "also committed" });
                return false;
            }
        );
        Assert.False(result);
        Assert.Equal(2UL, test.Db.Entries.Count);
        var db = test.Db;
        test.Dispose();
        Assert.Throws<ObjectDisposedException>(() => db.Entries.Count);
        Assert.Throws<ObjectDisposedException>(() => test.WithReducerTx(auth, _ => { }));
    }

    [Fact]
    public void FixturesHaveIndependentClockAndRandomness()
    {
        using var first = ModuleTestContext.Create(typeof(Entry).Assembly);
        using var second = ModuleTestContext.Create(typeof(Entry).Assembly);
        var auth = TestAuth.Internal();
        Assert.Equal(Timestamp.UNIX_EPOCH, first.Clock.Now);
        first.Clock.Set(new Timestamp(100));
        first.Clock.Advance(new TimeDuration(20));
        ReducerContext? captured = null;
        first.WithReducerTx(auth, ctx => captured = ctx);
        first.Clock.Advance(new TimeDuration(30));
        Assert.Equal(new Timestamp(120), captured!.Timestamp);
        Assert.Equal(Timestamp.UNIX_EPOCH, second.Clock.Now);

        first.Rng.SetSeed(42);
        var sequence = first.WithReducerTx(auth, ctx => (ctx.Rng.Next(), ctx.Rng.Next()));
        first.Rng.SetSeed(42);
        Assert.Equal(sequence, first.WithReducerTx(auth, ctx => (ctx.Rng.Next(), ctx.Rng.Next())));
        first.Rng.SetSeed(0);
        Assert.Equal(
            second.WithReducerTx(auth, ctx => ctx.Rng.Next()),
            first.WithReducerTx(auth, ctx => ctx.Rng.Next())
        );

        first.Dispose();
        Assert.Throws<ObjectDisposedException>(() => first.Clock.Now);
        Assert.Throws<ObjectDisposedException>(() => first.Rng.SetSeed(1));
        Assert.Equal(Timestamp.UNIX_EPOCH, second.Clock.Now);
    }

    [Fact]
    public void InvalidModulesAndUnsupportedCallbacksAreRejected()
    {
        Assert.Throws<ArgumentException>(
            () => ModuleTestContext.Create(typeof(ModuleTestContextTests).Assembly)
        );
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var auth = TestAuth.Internal();
        var called = false;
        async void AsyncVoid(ReducerContext _)
        {
            called = true;
            await Task.CompletedTask;
        }
        Assert.Throws<ArgumentException>(() => test.WithReducerTx(auth, AsyncVoid));
        Assert.Throws<ArgumentException>(() =>
        {
            _ = test.WithReducerTx(
                auth,
                _ =>
                {
                    called = true;
                    return Task.CompletedTask;
                }
            );
        });
        void Sync(ReducerContext _) => called = true;
        var multicast = (Action<ReducerContext>)Sync;
        multicast += Sync;
        Assert.Throws<ArgumentException>(() => test.WithReducerTx(auth, multicast));
        Assert.False(called);

        Assert.Throws<ArgumentException>(
            () =>
                test.WithReducerTx<object>(
                    auth,
                    ctx =>
                    {
                        ctx.Db.Entries.Insert(new Entry { Text = "erased awaitable" });
                        return Task.CompletedTask;
                    }
                )
        );
        Assert.Equal(0UL, test.Db.Entries.Count);
        test.WithReducerTx(auth, ctx => ctx.Db.Entries.Insert(new Entry { Text = "still usable" }));
        Assert.Equal(1UL, test.Db.Entries.Count);
    }
}
