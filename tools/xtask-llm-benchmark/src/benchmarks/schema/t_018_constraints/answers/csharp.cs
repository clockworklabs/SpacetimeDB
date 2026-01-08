using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Name = "accounts", Public = true)]
    [SpacetimeDB.Index.BTree(Name = "by_name", Columns = [nameof(Name)])]
    public partial struct Account
    {
        [SpacetimeDB.PrimaryKey] public int Id;
        [SpacetimeDB.Unique] public string Email;
        public string Name;
    }

    [SpacetimeDB.Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.accounts.Insert(new Account { Id = 1, Email = "a@example.com", Name = "Alice" });
        ctx.Db.accounts.Insert(new Account { Id = 2, Email = "b@example.com", Name = "Bob" });
    }
}