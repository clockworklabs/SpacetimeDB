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
    public static void Crud(ReducerContext ctx)
    {
        ctx.Db.User.Insert(new User { Id = 1, Name = "Alice", Age = 30, Active = true });
        ctx.Db.User.Insert(new User { Id = 2, Name = "Bob",   Age = 22, Active = false });
        ctx.Db.User.Id.Update(new User { Id = 1, Name = "Alice2", Age = 31, Active = false });
        ctx.Db.User.Id.Delete(2);
    }
}
