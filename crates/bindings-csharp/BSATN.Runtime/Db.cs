namespace SpacetimeDB;

public abstract record DbContext<DbView>
    where DbView : class, new()
{
    public readonly DbView Db;

    public DbContext() => Db = new();

    public DbContext(DbView db) => Db = db;
}
