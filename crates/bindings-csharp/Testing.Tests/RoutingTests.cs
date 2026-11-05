namespace SpacetimeDB.Testing.Tests;

using SpacetimeDB.Internal;
using TestingModule;

public class RoutingTests
{
    [Fact]
    public void UnboundHandlesRejectDatabaseAccess()
    {
        var table = new SpacetimeDB.Local().Entries;
        Assert.Throws<InvalidOperationException>(() => table.Count);
        Assert.Throws<InvalidOperationException>(
            () => table.Insert(new Entry { Text = "unbound" })
        );
        Assert.Throws<InvalidOperationException>(() => table.Id.Find(1));
        Assert.Throws<InvalidOperationException>(() => new LocalReadOnly().Entries.Count);
        Assert.Throws<InvalidOperationException>(() => default(QueryBuilder).Entries());
    }
}
