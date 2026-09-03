using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Widget", Public = true)]
    public partial struct Widget
    {
        [PrimaryKey] public ulong Id;
        public string Name;
        [Default(true)] public bool Enabled;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.Widget.Insert(new Widget { Id = 1, Name = "legacy", Enabled = true });
    }

    [Reducer]
    public static void Touch(ReducerContext ctx) { }
}
