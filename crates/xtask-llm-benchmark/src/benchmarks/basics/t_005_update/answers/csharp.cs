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

    [Reducer]
    public static void UpdateUser(ReducerContext ctx, int id, string name, int age, bool active)
    {
        ctx.Db.users.Id.Update(new User { Id = id, Name = name, Age = age, Active = active });
    }
}
