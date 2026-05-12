using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "User")]
    public partial struct User
    {
        [PrimaryKey, AutoInc] public ulong Id;
        public string Name;
        public int Age;
        public bool Active;
    }

    [Table(Accessor = "Result")]
    public partial struct Result
    {
        [PrimaryKey] public ulong Id;
        public string Name;
    }

    [Reducer]
    public static void InsertUser(ReducerContext ctx, string name, int age, bool active)
    {
        ctx.Db.User.Insert(new User { Id = 0, Name = name, Age = age, Active = active });
    }

    [Reducer]
    public static void LookupUserName(ReducerContext ctx, ulong id)
    {
        var u = ctx.Db.User.Id.Find(id);
        if (u.HasValue)
        {
            var row = u.Value;
            ctx.Db.Result.Insert(new Result { Id = row.Id, Name = row.Name });
        }
    }
}
