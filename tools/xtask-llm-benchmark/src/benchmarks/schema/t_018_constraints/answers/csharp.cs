using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "Account", Public = true)]
    [SpacetimeDB.Index.BTree(Accessor = "by_name", Columns = [nameof(Name)])]
    public partial struct Account
    {
        [SpacetimeDB.PrimaryKey, SpacetimeDB.AutoInc] public ulong Id;
        [SpacetimeDB.Unique] public string Email;
        public string Name;
    }

    [SpacetimeDB.Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.Account.Insert(new Account { Id = 0, Email = "a@example.com", Name = "Alice" });
        ctx.Db.Account.Insert(new Account { Id = 0, Email = "b@example.com", Name = "Bob" });
    }
}
