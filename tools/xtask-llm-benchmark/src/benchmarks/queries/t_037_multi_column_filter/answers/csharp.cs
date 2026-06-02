using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "EventLog")]
    [SpacetimeDB.Index.BTree(Accessor = "by_category_severity", Columns = new[] { nameof(EventLog.Category), nameof(EventLog.Severity) })]
    public partial struct EventLog
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public string Category;
        public uint Severity;
        public string Message;
    }

    [Table(Accessor = "FilteredEvent")]
    public partial struct FilteredEvent
    {
        [PrimaryKey]
        public ulong EventId;
        public string Message;
    }

    [Reducer]
    public static void FilterEvents(ReducerContext ctx, string category, uint severity)
    {
        foreach (var e in ctx.Db.EventLog.Iter())
        {
            if (e.Category == category && e.Severity == severity)
            {
                ctx.Db.FilteredEvent.Insert(new FilteredEvent
                {
                    EventId = e.Id,
                    Message = e.Message,
                });
            }
        }
    }
}
