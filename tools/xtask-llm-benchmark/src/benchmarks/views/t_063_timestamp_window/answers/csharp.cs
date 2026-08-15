using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Event", Public = true)]
    public partial struct Event
    {
        [PrimaryKey] public ulong Id;
        [SpacetimeDB.Index.BTree] public Timestamp OccurredAt;
        public string Label;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx)
    {
        long[] times = [100, 200, 300, 400, 500];
        for (var index = 0; index < times.Length; index++)
        {
            var micros = times[index];
            ctx.Db.Event.Insert(new Event { Id = (ulong)index + 1, OccurredAt = new Timestamp(micros), Label = $"event-{micros}" });
        }
    }

    [SpacetimeDB.View(Accessor = "WindowEvent", Public = true)]
    public static IEnumerable<Event> WindowEvent(AnonymousViewContext ctx) =>
        ctx.Db.Event.OccurredAt.Filter((new Timestamp(200), new Timestamp(400)));
}
