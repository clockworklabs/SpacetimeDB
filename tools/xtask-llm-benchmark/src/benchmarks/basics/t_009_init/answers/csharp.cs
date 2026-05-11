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

    [Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        ctx.Db.User.Insert(new User { Id = 0, Name = "Alice", Age = 30, Active = true });
        ctx.Db.User.Insert(new User { Id = 0, Name = "Bob",   Age = 22, Active = false });
    }
}
