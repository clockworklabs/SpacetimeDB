namespace SpacetimeDB.Testing.Tests;

using SelectionLeaf;
using TestingModule;

public class MountedTableTests
{
    [Fact]
    public void StandaloneTablesAndIndexesStayInTheirSelectedMount()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        var left = test.Db.Left.Leaf.Items;
        var right = test.Db.Right.Leaf.Items;
        var leftRow = left.Insert(
            new Item
            {
                Id = 1,
                Group = 10,
                Value = "left",
            }
        );
        var rightRow = right.Insert(
            new Item
            {
                Id = 1,
                Group = 10,
                Value = "right",
            }
        );
        Assert.Equal(1UL, left.Count);
        Assert.Equal(1UL, right.Count);
        Assert.Equal("left", left.Id.Find(1)!.Value.Value);
        Assert.Equal("right", right.Id.Find(1)!.Value.Value);

        leftRow.Value = "updated";
        Assert.Equal(leftRow, left.Id.Update(leftRow));
        Assert.Equal(leftRow, Assert.Single(left.Iter()));
        Assert.Equal(rightRow, Assert.Single(right.Iter()));
        Assert.Equal(leftRow, Assert.Single(left.Group.Filter(10u)));
        Assert.Equal(
            rightRow,
            Assert.Single(right.Group.Filter(new SpacetimeDB.Bound<uint>(5, 15)))
        );

        var view = test.CreateViewContext(TestAuth.Internal());
        Assert.Throws<InvalidOperationException>(() => Marker.Find(view, 1));
        Assert.Equal(leftRow, Marker.Find(view.As.Left.Leaf, 1));
        Assert.Equal(rightRow, Marker.Find(view.As.Right.Leaf, 1));
        Assert.Equal(1UL, view.Db.Left.Leaf.Items.Count);
        Assert.Equal(rightRow, Assert.Single(view.Db.Right.Leaf.Items.Group.Filter(10u)));
        var anonymous = test.CreateAnonymousViewContext();
        Assert.Equal(leftRow, anonymous.Db.Left.Leaf.Items.Id.Find(1));
        Assert.Equal(rightRow, Assert.Single(anonymous.Db.Right.Leaf.Items.Group.Filter(10u)));

        Assert.True(left.Id.Delete(1));
        Assert.Equal(0UL, left.Count);
        Assert.Equal(1UL, right.Count);
        left.Insert(leftRow);
        Assert.Equal(1UL, right.Group.Delete(new SpacetimeDB.Bound<uint>(5, 15)));
        Assert.Equal(1UL, left.Count);
        Assert.Equal(0UL, right.Count);
        right.Insert(rightRow);
        Assert.True(left.Delete(leftRow));
        Assert.Equal(1UL, right.Clear());
        Assert.Equal(0UL, left.Count);
        Assert.Equal(0UL, right.Count);
    }

    [Fact]
    public void SelectedTransactionsCommitRollbackAndExpireIndependently()
    {
        using var test = ModuleTestContext.Create(typeof(Entry).Assembly);
        test.WithReducerTx(
            TestAuth.Internal(),
            ctx =>
            {
                Marker.Insert(
                    ctx.As.Left.Leaf,
                    new Item
                    {
                        Id = 1,
                        Group = 1,
                        Value = "left",
                    }
                );
                Marker.Insert(
                    ctx.As.Right.Leaf,
                    new Item
                    {
                        Id = 1,
                        Group = 1,
                        Value = "right",
                    }
                );
            }
        );
        Func<Item?>? escaped = null;
        Assert.Throws<InvalidOperationException>(
            () =>
                test.WithReducerTx(
                    TestAuth.Internal(),
                    ctx =>
                    {
                        var index = ctx.Db.Left.Leaf.Items.Id;
                        escaped = () => index.Find(1);
                        index.Update(
                            new Item
                            {
                                Id = 1,
                                Group = 2,
                                Value = "rollback",
                            }
                        );
                        Assert.Equal(1UL, ctx.Db.Right.Leaf.Items.Group.Delete(1u));
                        throw new InvalidOperationException("rollback");
                    }
                )
        );
        Assert.Equal("left", test.Db.Left.Leaf.Items.Id.Find(1)!.Value.Value);
        Assert.Equal("right", test.Db.Right.Leaf.Items.Id.Find(1)!.Value.Value);
        Assert.Throws<ObjectDisposedException>(() => escaped!());

        test.CreateProcedureContext(TestAuth.Internal())
            .WithTx(tx =>
            {
                tx.Db.Left.Leaf.Items.Id.Update(
                    new Item
                    {
                        Id = 1,
                        Group = 2,
                        Value = "procedure",
                    }
                );
                Assert.Equal("right", tx.Db.Right.Leaf.Items.Id.Find(1)!.Value.Value);
                return 0;
            });
        Assert.Equal("procedure", test.Db.Left.Leaf.Items.Id.Find(1)!.Value.Value);
    }

    [Fact]
    public void IdenticalMountsInDifferentFixturesDoNotShareCachesOrRows()
    {
        using var first = ModuleTestContext.Create(typeof(Entry).Assembly);
        using var second = ModuleTestContext.Create(typeof(Entry).Assembly);
        first.Db.Left.Leaf.Items.Insert(new Item { Id = 1, Value = "first" });
        second.Db.Left.Leaf.Items.Insert(new Item { Id = 1, Value = "second" });
        Assert.Equal("first", first.Db.Left.Leaf.Items.Id.Find(1)!.Value.Value);
        Assert.Equal("second", second.Db.Left.Leaf.Items.Id.Find(1)!.Value.Value);
        Assert.Equal(0UL, first.Db.Right.Leaf.Items.Count);
        Assert.Equal(0UL, second.Db.Right.Leaf.Items.Count);
        var table = first.Db.Left.Leaf.Items;
        first.Dispose();
        Assert.Throws<ObjectDisposedException>(() => table.Count);
        Assert.Equal("second", second.Db.Left.Leaf.Items.Id.Find(1)!.Value.Value);
    }
}
