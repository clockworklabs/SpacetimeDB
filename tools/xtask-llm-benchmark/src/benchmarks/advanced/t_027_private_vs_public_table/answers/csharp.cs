using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "UserInternal")]
    public partial struct UserInternal
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public string Name;
        public string Email;
        public string PasswordHash;
    }

    [Table(Accessor = "UserPublic", Public = true)]
    public partial struct UserPublic
    {
        [PrimaryKey]
        public ulong Id;
        public string Name;
    }

    [Reducer]
    public static void RegisterUser(ReducerContext ctx, string name, string email, string passwordHash)
    {
        var intern = ctx.Db.UserInternal.Insert(new UserInternal
        {
            Id = 0,
            Name = name,
            Email = email,
            PasswordHash = passwordHash,
        });
        ctx.Db.UserPublic.Insert(new UserPublic
        {
            Id = intern.Id,
            Name = name,
        });
    }
}
