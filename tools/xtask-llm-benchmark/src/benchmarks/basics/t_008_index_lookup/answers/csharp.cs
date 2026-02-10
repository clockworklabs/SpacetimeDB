using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "User")]
    public partial struct User
    {
        [PrimaryKey] public int Id;
        public string Name;
        public int Age;
        public bool Active;
    }

    [Table(Name = "Result")]
    public partial struct Result
    {
        [PrimaryKey] public int Id;
        public string Name;
    }

    [Reducer]
    public static void LookupUserName(ReducerContext ctx, int id)
    {
        var u = ctx.Db.User.Id.Find(id);
        if (u.HasValue)
        {
            var row = u.Value;
            ctx.Db.Result.Insert(new Result { Id = row.Id, Name = row.Name });
        }
    }
}
