namespace SpacetimeDB.Sdk.Test.ViewPk;

using SpacetimeDB;

[Table(Accessor = "view_pk_player", Public = true)]
public partial struct ViewPkPlayer
{
    [PrimaryKey]
    public ulong id;
    public string name;
}

[Table(Accessor = "view_pk_membership", Public = true)]
public partial struct ViewPkMembership
{
    [PrimaryKey]
    public ulong id;
    [Index.BTree]
    public ulong player_id;
}

public static partial class Module
{
    [Reducer]
    public static void insert_view_pk_player(ReducerContext ctx, ulong id, string name)
    {
        ctx.Db.view_pk_player.Insert(new ViewPkPlayer { id = id, name = name });
    }

    [Reducer]
    public static void update_view_pk_player(ReducerContext ctx, ulong id, string name)
    {
        var old = ctx.Db.view_pk_player.id.Find(id);
        if (old != null)
        {
            ctx.Db.view_pk_player.id.Delete(id);
        }
        ctx.Db.view_pk_player.Insert(new ViewPkPlayer { id = id, name = name });
    }

    [Reducer]
    public static void insert_view_pk_membership(ReducerContext ctx, ulong id, ulong player_id)
    {
        ctx.Db.view_pk_membership.Insert(new ViewPkMembership { id = id, player_id = player_id });
    }

    [View(Accessor = "all_view_pk_players", Public = true)]
    public static IQuery<ViewPkPlayer> all_view_pk_players(ViewContext ctx)
    {
        return ctx.From.view_pk_player();
    }
}
