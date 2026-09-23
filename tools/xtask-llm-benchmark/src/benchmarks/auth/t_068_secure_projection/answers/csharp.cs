using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "SecretNote")]
    public partial struct SecretNote
    {
        [PrimaryKey] public ulong Id;
        [SpacetimeDB.Index.BTree] public Identity Owner;
        public string Title;
        public string SecretBody;
    }

    [SpacetimeDB.Type]
    public partial struct SafeNote { public ulong Id; public string Title; }

    [Reducer]
    public static void SeedPrivateNote(ReducerContext ctx) => ctx.Db.SecretNote.Insert(new SecretNote
    {
        Id = 1, Owner = ctx.Sender, Title = "Visible title", SecretBody = "never expose this",
    });

    [SpacetimeDB.View(Accessor = "MySafeNote", Public = true)]
    public static IEnumerable<SafeNote> MySafeNote(ViewContext ctx) =>
        ctx.Db.SecretNote.Owner.Filter(ctx.Sender).Select(note => new SafeNote { Id = note.Id, Title = note.Title });
}
