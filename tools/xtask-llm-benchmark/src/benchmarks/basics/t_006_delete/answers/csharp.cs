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

    [Reducer]
    public static void DeleteUser(ReducerContext ctx, int id)
    {
        ctx.Db.User.Id.Delete(id);
    }
}
