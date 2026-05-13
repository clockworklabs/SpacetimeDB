using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "User")]
    public partial struct User
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public string Name;
        public bool Active;
    }

    [Table(Accessor = "UserStats")]
    public partial struct UserStats
    {
        [PrimaryKey]
        public string Key;
        public ulong Count;
    }

    [Reducer]
    public static void ComputeUserCounts(ReducerContext ctx)
    {
        ulong total = 0;
        ulong active = 0;
        foreach (var u in ctx.Db.User.Iter())
        {
            total++;
            if (u.Active)
            {
                active++;
            }
        }

        ctx.Db.UserStats.Insert(new UserStats { Key = "total", Count = total });
        ctx.Db.UserStats.Insert(new UserStats { Key = "active", Count = active });
    }
}
