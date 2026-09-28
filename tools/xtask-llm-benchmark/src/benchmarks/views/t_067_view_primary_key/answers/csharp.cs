using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "SourceRow", Public = true)]
    public partial struct SourceRow
    {
        [PrimaryKey] public ulong Id;
        public string Value;
        [SpacetimeDB.Index.BTree] public bool Visible;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.SourceRow.Insert(new SourceRow { Id = 1, Value = "shown", Visible = true });
        ctx.Db.SourceRow.Insert(new SourceRow { Id = 2, Value = "hidden", Visible = false });
    }

    [SpacetimeDB.View(Accessor = "SourceView", Public = true, PrimaryKey = nameof(SourceRow.Id))]
    public static IEnumerable<SourceRow> SourceView(AnonymousViewContext ctx) =>
        ctx.Db.SourceRow.Visible.Filter(true);
}
