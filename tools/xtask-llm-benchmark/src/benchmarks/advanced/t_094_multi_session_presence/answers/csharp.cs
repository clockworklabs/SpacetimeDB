using System;
using System.Linq;
using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "LiveSession", Public = true)]
    public partial struct LiveSession
    {
        [PrimaryKey]
        public ConnectionId ConnectionId;
        public Identity Owner;
    }

    [Table(Accessor = "OnlineUser", Public = true)]
    public partial struct OnlineUser
    {
        [PrimaryKey]
        public Identity Owner;
        public ulong Connections;
    }

    [Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        var connectionId = ctx.ConnectionId ?? throw new Exception("connection missing");
        var owner = ctx.Sender;
        ctx.Db.LiveSession.Insert(new LiveSession { ConnectionId = connectionId, Owner = owner });
        if (ctx.Db.OnlineUser.Owner.Find(owner) is OnlineUser row)
        {
            row.Connections++;
            ctx.Db.OnlineUser.Owner.Update(row);
        }
        else
            ctx.Db.OnlineUser.Insert(new OnlineUser { Owner = owner, Connections = 1 });
    }

    [Reducer(ReducerKind.ClientDisconnected)]
    public static void ClientDisconnected(ReducerContext ctx)
    {
        var connectionId = ctx.ConnectionId ?? throw new Exception("connection missing");
        if (ctx.Db.LiveSession.ConnectionId.Find(connectionId) is LiveSession session)
        {
            ctx.Db.LiveSession.ConnectionId.Delete(connectionId);
            if (ctx.Db.OnlineUser.Owner.Find(session.Owner) is OnlineUser row)
            {
                if (row.Connections == 1)
                    ctx.Db.OnlineUser.Owner.Delete(session.Owner);
                else
                {
                    row.Connections--;
                    ctx.Db.OnlineUser.Owner.Update(row);
                }
            }
        }
    }
}
