namespace SpacetimeDB.Testing.Tests;

using TestingModule;

public class RangeIndexTests
{
    [Fact]
    public void InclusiveRangesAndCompositePrefixesUseTheOwningTransaction()
    {
        using var test = ModuleTestContext.Create(typeof(Point).Assembly);
        test.WithReducerTx(
            TestAuth.Internal(),
            ctx =>
            {
                (uint Id, uint Group, uint Score)[] points =
                [
                    (1, 1, 10),
                    (2, 1, 20),
                    (3, 1, 20),
                    (4, 1, 30),
                    (5, 2, 20),
                ];
                foreach (var (id, group, score) in points)
                    ctx.Db.Points.Insert(
                        new Point
                        {
                            Id = id,
                            Group = group,
                            Score = score,
                        }
                    );
            }
        );

        Assert.Equal([1u, 2u, 3u, 4u], Ids(test.Db.Points.ByGroupScore.Filter(1u)));
        Assert.Equal([2u, 3u], Ids(test.Db.Points.ByGroupScore.Filter((1u, 20u))));
        Assert.Equal(
            [2u, 3u, 4u],
            Ids(test.Db.Points.ByGroupScore.Filter((1u, new SpacetimeDB.Bound<uint>(20, 30))))
        );
        Assert.Equal(
            [3u],
            Ids(test.Db.Points.ByGroupScore.Filter((1u, 20u, new SpacetimeDB.Bound<uint>(3, 4))))
        );
        Assert.Equal([2u, 3u, 5u], Ids(test.Db.Points.Score.Filter(20u)));
        Assert.Empty(test.Db.Points.Score.Filter(new SpacetimeDB.Bound<uint>(40, 50)));

        Assert.Throws<InvalidOperationException>(
            () =>
                test.WithReducerTx(
                    TestAuth.Internal(),
                    ctx =>
                    {
                        Assert.Equal(2UL, ctx.Db.Points.ByGroupScore.Delete((1u, 20u)));
                        Assert.Empty(ctx.Db.Points.ByGroupScore.Filter((1u, 20u)));
                        Assert.Equal(0UL, ctx.Db.Points.ByGroupScore.Delete((1u, 20u)));
                        throw new InvalidOperationException("rollback");
                    }
                )
        );
        Assert.Equal(5UL, test.Db.Points.Count);
        Assert.Equal(4UL, test.Db.Points.Score.Delete(new SpacetimeDB.Bound<uint>(10, 20)));
        Assert.Equal(4u, Assert.Single(test.Db.Points.Iter()).Id);
        test.WithReducerTx(
            TestAuth.Internal(),
            ctx => Assert.Equal(1UL, ctx.Db.Points.ByGroupScore.Delete(1u))
        );
        Assert.Equal(0UL, test.Db.Points.Count);
    }

    [Fact]
    public void RangeIteratorsAndHandlesRespectTheirOwnerLifetime()
    {
        using var test = ModuleTestContext.Create(typeof(Point).Assembly);
        for (uint id = 1; id <= 2; id++)
            test.Db.Points.Insert(
                new Point
                {
                    Id = id,
                    Group = 1,
                    Score = 20,
                }
            );

        using var escaped = test.WithReducerTx(
            TestAuth.Internal(),
            ctx =>
            {
                var iterator = ctx.Db.Points.ByGroupScore.Filter(1u).GetEnumerator();
                Assert.True(iterator.MoveNext());
                return iterator;
            }
        );
        test.WithReducerTx(
            TestAuth.Internal(),
            ctx => Assert.Throws<ObjectDisposedException>(() => escaped.MoveNext())
        );

        var index = test.WithReducerTx(TestAuth.Internal(), ctx => ctx.Db.Points.ByGroupScore);
        Assert.Throws<ObjectDisposedException>(() => index.Filter(1u).ToArray());
        Assert.Throws<ObjectDisposedException>(() => index.Delete(1u));
    }

    private static uint[] Ids(IEnumerable<Point> rows) => [.. rows.Select(row => row.Id).Order()];
}
