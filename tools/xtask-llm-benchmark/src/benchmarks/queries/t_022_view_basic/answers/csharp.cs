using SpacetimeDB;
using System.Collections.Generic;
using System.Linq;

public static partial class Module
{
    [Table(Accessor = "Announcement", Public = true)]
    [SpacetimeDB.Index.BTree(Accessor = "Active", Columns = new[] { nameof(Announcement.Active) })]
    public partial struct Announcement
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public string Message;
        public bool Active;
    }

    [SpacetimeDB.View(Accessor = "ActiveAnnouncements", Public = true)]
    public static List<Announcement> ActiveAnnouncements(AnonymousViewContext ctx)
    {
        return ctx.Db.Announcement.Active.Filter(true).ToList();
    }
}
