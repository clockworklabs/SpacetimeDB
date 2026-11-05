namespace SpacetimeDB.Testing.Tests;

using SpacetimeDB.Internal;
using TestingModule;

public class UniqueIndexTests
{
    [Fact]
    public void StructIndexesUseTheOwningTransaction()
    {
        using var test = TestContext.Create(typeof(Entry).Assembly);
        var first = test.Db.Entries.Insert(new Entry { Text = "first" });
        var second = test.Db.Entries.Insert(new Entry { Text = "second" });
        Assert.Null(test.Db.Entries.Id.Find(0));
        var updated = test.Db.Entries.Id.Update(new Entry { Id = first.Id, Text = "updated" });
        Assert.Equal("updated", updated.Text);
        Assert.Equal(updated, test.Db.Entries.Id.Find(first.Id));

        ReducerContext? escaped = null;
        Assert.Throws<InvalidOperationException>(
            () =>
                test.WithReducerTx(
                    TestAuth.Internal(),
                    ctx =>
                    {
                        escaped = ctx;
                        ctx.Db.Entries.Id.Update(new Entry { Id = first.Id, Text = "rolled back" });
                        Assert.Equal("rolled back", ctx.Db.Entries.Id.Find(first.Id)!.Value.Text);
                        Assert.True(ctx.Db.Entries.Id.Delete(second.Id));
                        Assert.Null(ctx.Db.Entries.Id.Find(second.Id));
                        throw new InvalidOperationException("rollback");
                    }
                )
        );
        Assert.Equal(updated, test.Db.Entries.Id.Find(first.Id));
        Assert.Equal(second, test.Db.Entries.Id.Find(second.Id));
        Assert.Throws<ObjectDisposedException>(() => escaped!.Db.Entries.Id.Find(first.Id));
        Assert.Throws<ObjectDisposedException>(() => escaped!.Db.Entries.Id.Update(first));
        Assert.Throws<ObjectDisposedException>(() => escaped!.Db.Entries.Id.Delete(first.Id));

        test.WithReducerTx(
            TestAuth.Internal(),
            ctx =>
            {
                ctx.Db.Entries.Id.Update(first);
                Assert.True(ctx.Db.Entries.Id.Delete(second.Id));
            }
        );
        Assert.Equal(first, test.Db.Entries.Id.Find(first.Id));
        Assert.Null(test.Db.Entries.Id.Find(second.Id));
        Assert.True(test.Db.Entries.Id.Delete(first.Id));
        Assert.False(test.Db.Entries.Id.Delete(first.Id));
        var index = test.Db.Entries.Id;
        test.Dispose();
        Assert.Throws<ObjectDisposedException>(() => index.Find(first.Id));
    }

    [Fact]
    public void ClassIndexesPreserveConstraintsAndGeneratedColumns()
    {
        using var test = TestContext.Create(typeof(Account).Assembly);
        var first = test.Db.Accounts.Insert(new Account { Id = 1, Name = "first" });
        test.Db.Accounts.Insert(new Account { Id = 2, Name = "second" });
        var updated = test.Db.Accounts.Id.Update(new Account { Id = 1, Name = "updated" });
        Assert.True(updated.Serial > first.Serial);
        Assert.Equal(updated.Serial, test.Db.Accounts.Id.Find(1)!.Serial);
        Assert.Null(test.Db.Accounts.Name.Find("first"));
        Assert.Equal(updated.Id, test.Db.Accounts.Name.Find("updated")!.Id);
        Assert.Throws<UniqueConstraintViolationException>(
            () =>
                test.Db.Accounts.Id.Update(
                    new Account
                    {
                        Id = 1,
                        Serial = updated.Serial,
                        Name = "second",
                    }
                )
        );
        Assert.Equal("updated", test.Db.Accounts.Id.Find(1)!.Name);

        var readOnly = new LocalReadOnly(test.Db.Access!);
        Assert.Equal(updated.Serial, readOnly.Accounts.Name.Find("updated")!.Serial);
        Assert.Null(readOnly.Accounts.Name.Find("missing"));
        Assert.True(test.Db.Accounts.Name.Delete("updated"));
        Assert.False(test.Db.Accounts.Name.Delete("updated"));
        Assert.Equal(2UL, Assert.Single(test.Db.Accounts.Iter()).Id);
    }
}
