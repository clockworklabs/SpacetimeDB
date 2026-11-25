namespace SpacetimeDB.Testing.Tests;

using SelectionLeaf;
using TestingModule;

public class MountedTableTests
{
    [Fact]
    public void TablesAndIndexesStayInTheirSelectedMount()
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
        leftRow.Value = "updated";
        left.Id.Update(leftRow);
        Assert.Equal(leftRow, left.Id.Find(1));
        Assert.Equal(rightRow, Assert.Single(right.Group.Filter(10u)));

        var view = test.CreateViewContext(TestAuth.Internal());
        Assert.Throws<InvalidOperationException>(() => Marker.Find(view, 1));
        Assert.Equal(leftRow, Marker.Find(view.As.Left.Leaf, 1));
        Assert.Equal(rightRow, test.CreateAnonymousViewContext().Db.Right.Leaf.Items.Id.Find(1));

        Assert.True(left.Id.Delete(1));
        Assert.Equal(0UL, left.Count);
        Assert.Equal(1UL, right.Count);
    }

    [Fact]
    public void TransactionsCommitAndRollBackAcrossMounts()
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
        Assert.Throws<InvalidOperationException>(
            () =>
                test.WithReducerTx(
                    TestAuth.Internal(),
                    ctx =>
                    {
                        ctx.Db.Left.Leaf.Items.Id.Update(
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
