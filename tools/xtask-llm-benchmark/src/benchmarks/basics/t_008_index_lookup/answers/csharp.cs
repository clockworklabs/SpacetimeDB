using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "users")]
    public partial struct User
    {
        [PrimaryKey] public int Id;
        public string Name;
        public int Age;
        public bool Active;
    }

    [Table(Name = "results")]
    public partial struct Result
    {
        [PrimaryKey] public int Id;
        public string Name;
    }

    [Reducer]
    public static void LookupUserName(ReducerContext ctx, int id)
    {
        var u = ctx.Db.users.Id.Find(id);
        if (u.HasValue)
        {
            var row = u.Value;
            ctx.Db.results.Insert(new Result { Id = row.Id, Name = row.Name });
        }
    }
}
