// C# port of idc-self-outbox: an outbox to a reducer in this module,
// declared without receiver bindings.
using SpacetimeDB;

[Type]
public partial struct State
{
    public ulong revision;
    public string? displayName;
}

[Type]
public partial record Operation : TaggedEnum<(State SetState, Unit ClearState)>;

[Type]
public partial struct Message
{
    public ulong ItemID;
    public List<Operation> operations;
}

[Table(Accessor = "delivered_ping")]
public partial struct DeliveredPing
{
    [PrimaryKey, AutoInc]
    public ulong id;
    public Message body;
}

[Table(Accessor = "outbound_ping", Outbox = typeof(Module), OutboxReducer = nameof(Module.receivePing))]
public partial struct OutboundPing
{
    [PrimaryKey, AutoInc]
    public ulong id;

    [Target]
    public Identity target;

    [Param("messageBody")]
    public Message payload;
}

[Table(Accessor = "ping_results")]
public partial struct PingResult
{
    [PrimaryKey]
    public ulong id;
    public Message body;
    public string? error;
}

public static partial class Module
{
    [SpacetimeDB.Settings]
    public const CaseConversionPolicy CASE_CONVERSION_POLICY = CaseConversionPolicy.None;

    [Reducer]
    public static void receivePing(ReducerContext ctx, Message messageBody) =>
        ctx.Db.delivered_ping.Insert(new DeliveredPing { id = 0, body = messageBody });

    [Reducer(Name = "WirePingResult")]
    [OnResult(typeof(OutboundPing))]
    public static void onPingResult(
        ReducerContext ctx,
        OutboundPing row,
        Result<Unit, string> result
    ) =>
        ctx.Db.ping_results.Insert(
            new PingResult
            {
                id = row.id,
                body = row.payload,
                error = result is Result<Unit, string>.ErrR(var error) ? error : null,
            }
        );

    [Reducer]
    public static void enqueuePing(ReducerContext ctx, Identity target, Message messageBody) =>
        ctx.Db.outbound_ping.Insert(
            new OutboundPing
            {
                id = 0,
                target = target,
                payload = messageBody,
            }
        );
}
