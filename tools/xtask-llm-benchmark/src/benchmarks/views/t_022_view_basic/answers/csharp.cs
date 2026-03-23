using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Player", Public = true)]
    public partial struct Player
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public string Name;
        public uint Score;
    }

    [SpacetimeDB.View(Accessor = "AllPlayers", Public = true)]
    public static List<Player> AllPlayers(AnonymousViewContext ctx)
    {
        var rows = new List<Player>();
        foreach (var p in ctx.Db.Player.Iter())
        {
            rows.Add(p);
        }
        return rows;
    }
}
