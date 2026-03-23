using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Account")]
    public partial struct Account
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        [Unique]
        public string Email;
        public string DisplayName;
    }

    [Reducer]
    public static void CreateAccount(ReducerContext ctx, string email, string displayName)
    {
        ctx.Db.Account.Insert(new Account
        {
            Id = 0,
            Email = email,
            DisplayName = displayName,
        });
    }
}
