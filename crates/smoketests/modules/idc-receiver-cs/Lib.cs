// C# port of idc-receiver-ts.
using SpacetimeDB;

[Type]
public partial struct PingDetails
{
    public List<string> Labels;
    public List<PingCheckpoint> Checkpoints;
    public string? Note;
}

[Type]
public partial struct PingCheckpoint
{
    public uint Sequence;
    public PingState State;
}

[Type]
public partial record PingState : TaggedEnum<(Unit Ready, string? Deferred)>;

[Type]
public enum PingPriority
{
    Low,
    High,
}

[Table(Accessor = "PingLog")]
public partial struct PingLog
{
    [PrimaryKey, AutoInc]
    public ulong Id;
    public Identity Sender;
    public uint ModuleVersion;
    public string Body;
}

[Table(Accessor = "DetailedPingLog")]
public partial struct DetailedPingLog
{
    [PrimaryKey, AutoInc]
    public ulong Id;
    public string Body;
    public PingDetails Details;
}

[Table(Accessor = "PriorityLog")]
public partial struct PriorityLog
{
    [PrimaryKey, AutoInc]
    public ulong Id;
    public PingPriority Priority;
}

public static partial class Module
{
    [Reducer(Name = "Receive_Ping")]
    public static void ReceivePing(ReducerContext ctx, string body)
    {
        if (body == "reject")
        {
            throw new Exception("rejected");
        }
        ctx.Db.PingLog.Insert(
            new PingLog
            {
                Sender = ctx.Sender,
                ModuleVersion = 1,
                Body = body,
            }
        );
    }

    // Same signature, different canonical-name casing. IDC must not dispatch here
    // when the sender's bindings specify Receive_Ping.
    [Reducer(Name = "receive_ping")]
    public static void ReceiveLowercasePing(ReducerContext ctx, string body) =>
        ctx.Db.PingLog.Insert(
            new PingLog
            {
                Sender = ctx.Sender,
                ModuleVersion = 1,
                Body = $"wrong-case:{body}",
            }
        );

    [Reducer(Name = "Receive_DetailedPing")]
    public static void ReceiveDetailedPing(ReducerContext ctx, string body, PingDetails details) =>
        ctx.Db.DetailedPingLog.Insert(new DetailedPingLog { Body = body, Details = details });

    [Reducer]
    public static void ReceivePriority(ReducerContext ctx, PingPriority priority) =>
        ctx.Db.PriorityLog.Insert(new PriorityLog { Priority = priority });
}
