namespace SpacetimeDB;

public abstract class DbContext<DbView>
    where DbView : struct
{
    public readonly DbView Db = new();
}
