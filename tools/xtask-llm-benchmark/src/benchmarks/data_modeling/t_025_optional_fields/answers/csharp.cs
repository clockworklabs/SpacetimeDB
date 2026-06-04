using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Player")]
    public partial struct Player
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public string Name;
        public string? Nickname;
        public uint? HighScore;
    }

    [Reducer]
    public static void CreatePlayer(ReducerContext ctx, string name, string? nickname, uint? highScore)
    {
        ctx.Db.Player.Insert(new Player
        {
            Id = 0,
            Name = name,
            Nickname = nickname,
            HighScore = highScore,
        });
    }
}
