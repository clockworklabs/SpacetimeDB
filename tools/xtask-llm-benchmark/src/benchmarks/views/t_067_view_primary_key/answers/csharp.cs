using SpacetimeDB;
public static partial class Module
{
    [Table(Accessor = "SourceRow", Public = true)] public partial struct SourceRow { [PrimaryKey] public ulong Id; public string Value; [SpacetimeDB.Index.BTree] public bool Visible; }
    [SpacetimeDB.View(Accessor = "SourceView", Public = true, PrimaryKey = nameof(SourceRow.Id))]
    public static IEnumerable<SourceRow> SourceView(AnonymousViewContext ctx) => ctx.Db.SourceRow.Visible.Filter(true);
}
