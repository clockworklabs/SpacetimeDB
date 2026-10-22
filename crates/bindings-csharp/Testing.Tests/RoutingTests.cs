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
        Assert.Throws<InvalidOperationException>(() => table.Iter().ToArray());
        Assert.Throws<InvalidOperationException>(
            () => table.Insert(new Entry { Text = "unbound" })
        );
        Assert.Throws<InvalidOperationException>(() => table.Delete(default));
        Assert.Throws<InvalidOperationException>(() => table.Clear());
        Assert.Throws<InvalidOperationException>(() => table.Id.Find(1));
        Assert.Throws<InvalidOperationException>(() => new LocalReadOnly().Entries.Count);
        Assert.Throws<InvalidOperationException>(() => new LocalReadOnly().Entries.Id.Find(1));
        Assert.Throws<InvalidOperationException>(() => default(QueryBuilder).Entries());
    }

    [Fact]
    public void GeneratedTablesUseStandaloneAndBoundTransactions()
    {
        using var backend = new DatabaseBackend(Schema.Build(), Schema.CreateNamespaces());
        var db = new SpacetimeDB.Local(new DatabaseAccess(backend));
        var inserted = db.Entries.Insert(new Entry { Text = "committed" });
        Assert.NotEqual(0UL, inserted.Id);
        Assert.Equal("committed", Assert.Single(db.Entries.Iter()).Text);
        Assert.Throws<UniqueConstraintViolationException>(
            () => db.Entries.Insert(new Entry { Id = inserted.Id, Text = "duplicate key" })
        );
        Assert.Equal(1UL, db.Entries.Count);

        using var transaction = backend.BeginTransaction();
        var local = new SpacetimeDB.Local(new DatabaseAccess(transaction));
        var table = local.Entries;
        table.Insert(new Entry { Text = "rolled back" });
        Assert.Equal(2UL, table.Count);
        using var iterator = table.Iter().GetEnumerator();
        Assert.True(iterator.MoveNext());
        var index = table.Id;
        Assert.Throws<NotSupportedException>(() => index.Find(inserted.Id));
        Assert.Throws<NotSupportedException>(() => index.Update(inserted));
        Assert.Throws<NotSupportedException>(() => table.Clear());
        Assert.Throws<NotSupportedException>(() => table.Delete(inserted));
        transaction.Rollback();
        using (var next = backend.BeginTransaction())
        {
            Assert.Throws<InvalidOperationException>(() => table.Count);
            Assert.Throws<InvalidOperationException>(() => iterator.MoveNext());
            Assert.Throws<InvalidOperationException>(() => index.Find(inserted.Id));
        }
        Assert.Equal(1UL, db.Entries.Count);

        using var fixtureIterator = db.Entries.Iter().GetEnumerator();
        Assert.True(fixtureIterator.MoveNext());
        backend.Dispose();
        Assert.Throws<ObjectDisposedException>(() => fixtureIterator.MoveNext());
        Assert.Throws<ObjectDisposedException>(() => db.Entries.Count);
    }

    [Fact]
    public void SameGeneratedHandlesResolveDifferentMountedRoots()
    {
        var (schemaA, namespacesA) = Schema.BuildMounted("first");
        var (schemaB, namespacesB) = Schema.BuildMounted("second");
        using var backendA = new DatabaseBackend(schemaA, namespacesA);
        using var backendB = new DatabaseBackend(schemaB, namespacesB);
        var dbA = new SpacetimeDB.Local(new DatabaseAccess(backendA));
        var dbB = new SpacetimeDB.Local(new DatabaseAccess(backendB));
        Parallel.Invoke(
            () => dbA.Entries.Insert(new Entry { Text = "A" }),
            () => dbB.Entries.Insert(new Entry { Text = "B" })
        );
        Assert.Equal("A", Assert.Single(dbA.Entries.Iter()).Text);
        Assert.Equal("B", Assert.Single(dbB.Entries.Iter()).Text);
        var readOnly = new LocalReadOnly(new DatabaseAccess(backendB));
        Assert.Equal(1UL, readOnly.Entries.Count);
        Assert.Throws<NotSupportedException>(() => readOnly.Entries.Id.Find(1));
        Assert.Equal(
            "SELECT * FROM \"second\".\"Entries\"",
            new QueryBuilder(readOnly).Entries().ToSql()
        );
    }
}
