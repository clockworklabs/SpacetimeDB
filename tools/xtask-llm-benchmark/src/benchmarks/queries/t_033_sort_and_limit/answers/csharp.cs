using SpacetimeDB;
using System.Linq;

public static partial class Module
{
    [Table(Accessor = "Player")]
    public partial struct Player
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public string Name;
        public ulong Score;
    }

    [Table(Accessor = "Leaderboard")]
    public partial struct LeaderboardEntry
    {
        [PrimaryKey]
        public uint Rank;
        public string PlayerName;
        public ulong Score;
    }

    [Reducer]
    public static void BuildLeaderboard(ReducerContext ctx, uint limit)
    {
        var players = ctx.Db.Player.Iter()
            .OrderByDescending(p => p.Score)
            .Take((int)limit)
            .ToList();

        for (int i = 0; i < players.Count; i++)
        {
            ctx.Db.Leaderboard.Insert(new LeaderboardEntry
            {
                Rank = (uint)(i + 1),
                PlayerName = players[i].Name,
                Score = players[i].Score,
            });
        }
    }
}
