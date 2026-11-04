using PackageModule;
using SpacetimeDB.Testing;
using Xunit;

public class PackageTests
{
    [Fact]
    public void PackagedFixtureCommitsRollsBackAndDisposes()
    {
        using var test = TestContext.Create(typeof(Module).Assembly);
        var auth = TestAuth.Internal();

        test.WithReducerTx(auth, ctx => Module.Add(ctx, 1, "committed"));
        Assert.Equal("committed", test.Db.Entries.Id.Find(1)!.Value.Text);

        Assert.Throws<ArgumentException>(() =>
            test.WithReducerTx(auth, ctx => Module.Add(ctx, 2, ""))
        );
        Assert.Null(test.Db.Entries.Id.Find(2));
        Assert.Equal("committed", Assert.Single(test.Db.Entries.Iter()).Text);
    }
}
