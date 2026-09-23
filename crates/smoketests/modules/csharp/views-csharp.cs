using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Table", Public = true)]
    public partial struct Table
    {
        public uint Value;
        public bool Alive;
    }

    [Reducer]
    public static void InsertValue(ReducerContext ctx, uint value, bool alive)
    {
        ctx.Db.Table.Insert(new Table { Value = value, Alive = alive });
    }

    [View(Accessor = "all", Public = true)]
    public static IQuery<Table> All(ViewContext ctx)
    {
        return ctx.From.Table();
    }

    [View(Accessor = "some", Public = true)]
    public static IQuery<Table> Some(ViewContext ctx)
    {
        return ctx.From.Table().Where(Row => Row.Alive);
    }
}
