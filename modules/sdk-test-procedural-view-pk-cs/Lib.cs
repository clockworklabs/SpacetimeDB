namespace SpacetimeDB.Sdk.Test.ProceduralViewPk;

using SpacetimeDB;
using System.Collections.Generic;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "left_source", Public = true)]
    public partial struct LeftSource
    {
        [SpacetimeDB.PrimaryKey]
        public ulong id;

        [SpacetimeDB.Index.BTree]
        public Identity sender;

        public ulong filter;
    }

    [SpacetimeDB.Table(Accessor = "right_source", Public = true)]
    public partial struct RightSource
    {
        [SpacetimeDB.PrimaryKey]
        public ulong id;

        [SpacetimeDB.Index.BTree]
        public Identity sender;

        public ulong filter;
    }

    [SpacetimeDB.Reducer]
    public static void insert_left(ReducerContext ctx, ulong id, ulong filter)
    {
        ctx.Db.left_source.Insert(
            new LeftSource
            {
                id = id,
                sender = ctx.Sender,
                filter = filter,
            }
        );
    }

    [SpacetimeDB.Reducer]
    public static void update_left(ReducerContext ctx, ulong id, ulong filter)
    {
        ctx.Db.left_source.id.Update(
            new LeftSource
            {
                id = id,
                sender = ctx.Sender,
                filter = filter,
            }
        );
    }

    [SpacetimeDB.Reducer]
    public static void insert_right(ReducerContext ctx, ulong id, ulong filter)
    {
        ctx.Db.right_source.Insert(
            new RightSource
            {
                id = id,
                sender = ctx.Sender,
                filter = filter,
            }
        );
    }

    [SpacetimeDB.View(Accessor = "sender_left_view", Public = true, PrimaryKey = "id")]
    public static IEnumerable<LeftSource> sender_left_view(ViewContext ctx)
    {
        return ctx.Db.left_source.sender.Filter(ctx.Sender);
    }

    [SpacetimeDB.View(Accessor = "sender_right_view", Public = true, PrimaryKey = "id")]
    public static IEnumerable<RightSource> sender_right_view(ViewContext ctx)
    {
        return ctx.Db.right_source.sender.Filter(ctx.Sender);
    }
}
