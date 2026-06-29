namespace SpacetimeDB.Sdk.Test.ViewPk;

using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "view_pk_player", Public = true)]
    public partial struct ViewPkPlayer
    {
        [SpacetimeDB.PrimaryKey]
        public ulong id;
        public string name;
    }

    [SpacetimeDB.Table(Accessor = "view_pk_membership", Public = true)]
    public partial struct ViewPkMembership
    {
        [SpacetimeDB.PrimaryKey]
        public ulong id;

        [SpacetimeDB.Index.BTree]
        public ulong player_id;
    }

    [SpacetimeDB.Table(Accessor = "view_pk_membership_secondary", Public = true)]
    public partial struct ViewPkMembershipSecondary
    {
        [SpacetimeDB.PrimaryKey]
        public ulong id;

        [SpacetimeDB.Index.BTree]
        public ulong player_id;
    }

    [SpacetimeDB.Reducer]
    public static void insert_view_pk_player(ReducerContext ctx, ulong id, string name)
    {
        ctx.Db.view_pk_player.Insert(new ViewPkPlayer { id = id, name = name });
    }

    [SpacetimeDB.Reducer]
    public static void update_view_pk_player(ReducerContext ctx, ulong id, string name)
    {
        ctx.Db.view_pk_player.id.Update(new ViewPkPlayer { id = id, name = name });
    }

    [SpacetimeDB.Reducer]
    public static void insert_view_pk_membership(ReducerContext ctx, ulong id, ulong player_id)
    {
        ctx.Db.view_pk_membership.Insert(new ViewPkMembership { id = id, player_id = player_id });
    }

    [SpacetimeDB.Reducer]
    public static void insert_view_pk_membership_secondary(
        ReducerContext ctx,
        ulong id,
        ulong player_id
    )
    {
        ctx.Db.view_pk_membership_secondary.Insert(
            new ViewPkMembershipSecondary { id = id, player_id = player_id }
        );
    }

    [SpacetimeDB.View(Accessor = "all_view_pk_players", Public = true)]
    public static IQuery<ViewPkPlayer> all_view_pk_players(ViewContext ctx)
    {
        return ctx.From.view_pk_player();
    }

    [SpacetimeDB.View(Accessor = "sender_view_pk_players_a", Public = true)]
    public static IQuery<ViewPkPlayer> sender_view_pk_players_a(ViewContext ctx)
    {
        return ctx
            .From.view_pk_membership()
            .RightSemijoin(
                ctx.From.view_pk_player(),
                (membership, player) => membership.player_id.Eq(player.id)
            );
    }

    [SpacetimeDB.View(Accessor = "sender_view_pk_players_b", Public = true)]
    public static IQuery<ViewPkPlayer> sender_view_pk_players_b(ViewContext ctx)
    {
        return ctx
            .From.view_pk_membership_secondary()
            .RightSemijoin(
                ctx.From.view_pk_player(),
                (membership, player) => membership.player_id.Eq(player.id)
            );
    }
}
