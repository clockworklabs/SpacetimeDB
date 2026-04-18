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

    [Reducer]
    public static void InsertUser(ReducerContext ctx, string name, int age, bool active)
    {
        ctx.Db.User.Insert(new User { Id = 0, Name = name, Age = age, Active = active });
    }

    [Reducer]
    public static void DeleteUser(ReducerContext ctx, ulong id)
    {
        ctx.Db.User.Id.Delete(id);
    }
}
