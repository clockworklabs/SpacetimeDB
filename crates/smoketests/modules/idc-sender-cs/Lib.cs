// C# port of idc-sender-ts. `spacetime generate` writes the receiver bindings to remote_bindings/.
using SpacetimeDB;
using Auth = RemoteBindings.Auth;
using GameWorld = RemoteBindings.GameWorld;

[Table(Accessor = "OutboundPing", Outbox = typeof(GameWorld.Reducers.ReceivePing))]
public partial struct OutboundPing
{
    [PrimaryKey, AutoInc]
    public ulong MsgId;

    [Target]
    public GameWorld.Identity Target;

    // The column name differs from the receiver's parameter.
    [Param("body")]
    public string Bodypo;
}

[Table(Accessor = "OutboundDetailedPing", Outbox = typeof(GameWorld.Reducers.ReceiveDetailedPing))]
public partial struct OutboundDetailedPing
{
    [PrimaryKey, AutoInc]
    public ulong MsgId;

    [Target]
    public GameWorld.Identity Target;
    public string Body;
    public GameWorld.PingDetails Details;
}

[Table(Accessor = "OutboundAuthDetails", Outbox = typeof(Auth.Reducers.ReceiveDetailedPing))]
public partial struct OutboundAuthDetails
{
    [PrimaryKey, AutoInc]
    public ulong MsgId;

    [Target]
    public Auth.Identity Target;
    public string Body;
    public Auth.PingDetails Details;
}

[Table(Accessor = "OutboundWorldPriority", Outbox = typeof(GameWorld.Reducers.ReceivePriority))]
public partial struct OutboundWorldPriority
{
    [PrimaryKey, AutoInc]
    public ulong MsgId;

    [Target]
    public GameWorld.Identity Target;
    public GameWorld.PingPriority Priority;
}

[Table(Accessor = "OutboundAuthPriority", Outbox = typeof(Auth.Reducers.ReceivePriority))]
public partial struct OutboundAuthPriority
{
    [PrimaryKey, AutoInc]
    public ulong MsgId;

    [Target]
    public Auth.Identity Target;
    public Auth.PingPriority Priority;
}

[Type]
public partial struct PingDetails
{
    public string Source;
}

[Table(Accessor = "SenderDetails")]
public partial struct SenderDetails
{
    [PrimaryKey]
    public ulong Id;
    public PingDetails Details;
}

[Table(Accessor = "PingResult")]
public partial struct PingResult
{
    [PrimaryKey]
    public ulong Id;

    [AutoInc]
    public ulong CallbackOrder;
    public uint ModuleVersion;
    public string Body;
    public bool Succeeded;
    public string? Error;
}

[Table(Accessor = "DetailedPingResult")]
public partial struct DetailedPingResult
{
    [PrimaryKey]
    public ulong Id;
    public string Body;
    public GameWorld.PingDetails Details;
}

public static partial class Module
{
    [Reducer]
    public static void EnqueuePing(ReducerContext ctx, GameWorld.Identity target, string body) =>
        ctx.Db.OutboundPing.Insert(new OutboundPing { Target = target, Bodypo = body });

    [Reducer]
    public static void EnqueuePings(
        ReducerContext ctx,
        GameWorld.Identity target,
        List<string> bodies
    )
    {
        foreach (var body in bodies)
        {
            EnqueuePing(ctx, target, body);
        }
    }

    [Reducer]
    public static void EnqueueDetailedPing(
        ReducerContext ctx,
        GameWorld.Identity target,
        string body
    ) =>
        ctx.Db.OutboundDetailedPing.Insert(
            new OutboundDetailedPing
            {
                Target = target,
                Body = body,
                Details = new(
                    ["nested", "idc"],
                    [
                        new(1, new GameWorld.PingState.Ready(default)),
                        new(2, new GameWorld.PingState.Deferred("waiting")),
                        new(3, new GameWorld.PingState.Deferred(null)),
                    ],
                    "optional-note"
                ),
            }
        );

    [Reducer]
    public static void EnqueueTwoDependencies(
        ReducerContext ctx,
        GameWorld.Identity world,
        Auth.Identity authTarget
    )
    {
        ctx.Db.OutboundDetailedPing.Insert(
            new OutboundDetailedPing
            {
                Target = world,
                Body = "world-dependency",
                Details = new(["world"], [], null),
            }
        );
        ctx.Db.OutboundAuthDetails.Insert(
            new OutboundAuthDetails
            {
                Target = authTarget,
                Body = "auth-dependency",
                Details = new(["auth"], [], null),
            }
        );
        ctx.Db.OutboundWorldPriority.Insert(
            new OutboundWorldPriority { Target = world, Priority = GameWorld.PingPriority.High }
        );
        ctx.Db.OutboundAuthPriority.Insert(
            new OutboundAuthPriority { Target = authTarget, Priority = Auth.PingPriority.Low }
        );
        ctx.Db.SenderDetails.Insert(
            new SenderDetails { Id = 1, Details = new PingDetails { Source = "sender" } }
        );
    }

    [Reducer]
    [OnResult(typeof(OutboundPing))]
    public static void OnPingResult(
        ReducerContext ctx,
        OutboundPing row,
        Result<Unit, string> result
    ) =>
        ctx.Db.PingResult.Insert(
            new PingResult
            {
                Id = row.MsgId,
                ModuleVersion = 1,
                Body = row.Bodypo,
                Succeeded = result is Result<Unit, string>.OkR,
                Error = result is Result<Unit, string>.ErrR(var error) ? error : null,
            }
        );

    [Reducer]
    [OnResult(typeof(OutboundDetailedPing))]
    public static void OnDetailedPingResult(
        ReducerContext ctx,
        OutboundDetailedPing row,
        Result<Unit, string> result
    )
    {
        if (result is Result<Unit, string>.ErrR(var error))
        {
            throw new Exception(error);
        }
        ctx.Db.DetailedPingResult.Insert(
            new DetailedPingResult
            {
                Id = row.MsgId,
                Body = row.Body,
                Details = row.Details,
            }
        );
    }
}
