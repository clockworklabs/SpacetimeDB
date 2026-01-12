using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "events")]
    public partial struct Event
    {
        [PrimaryKey, AutoInc] public int Id;
        public string Kind;
    }

    [Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        ctx.Db.events.Insert(new Event { Kind = "connected" });
    }

    [Reducer(ReducerKind.ClientDisconnected)]
    public static void ClientDisconnected(ReducerContext ctx)
    {
        ctx.Db.events.Insert(new Event { Kind = "disconnected" });
    }
}
