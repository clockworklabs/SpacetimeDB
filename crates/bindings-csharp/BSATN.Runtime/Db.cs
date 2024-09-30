namespace SpacetimeDB;

public abstract record DbContext<DbView>(DbView Db)
    where DbView : class, new()
{
    public DbContext()
        : this(new DbView()) { }
}
