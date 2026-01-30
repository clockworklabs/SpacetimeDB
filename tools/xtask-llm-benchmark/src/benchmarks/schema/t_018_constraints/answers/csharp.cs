using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Name = "Account", Public = true)]
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
        ctx.Db.Account.Insert(new Account { Id = 1, Email = "a@example.com", Name = "Alice" });
        ctx.Db.Account.Insert(new Account { Id = 2, Email = "b@example.com", Name = "Bob" });
    }
}
