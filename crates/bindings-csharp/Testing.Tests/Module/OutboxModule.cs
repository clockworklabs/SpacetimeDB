namespace TestingModule;

using SpacetimeDB;
using Receiver = SpacetimeDB.Modules.ModuleTestCs.Receiver;

[Table(Accessor = "PingOutbox", Outbox = typeof(Receiver.Reducers.ReceivePing))]
public partial struct PingOutboxRow
{
    [PrimaryKey, AutoInc]
    public ulong Id;

    [Target]
    public Receiver.Identity Target;

    public string Body;
}

public static partial class OutboxModule
{
    [Reducer]
    [OnResult(typeof(PingOutboxRow))]
    public static void OnPingResult(
        ReducerContext ctx,
        PingOutboxRow row,
        Result<Unit, string> result
    ) { }
}
