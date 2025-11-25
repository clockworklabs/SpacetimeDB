// Server module for regression tests.
// Everything we're testing for happens SDK-side so this module is very uninteresting.

using SpacetimeDB;

[SpacetimeDB.Type]
public partial class ReturnStruct
{
    public uint A;
    public string B;

    public ReturnStruct(uint a, string b)
    {
        A = a;
        B = b;
    }

    public ReturnStruct()
    {
        A = 0;
        B = string.Empty;
    }
}

[SpacetimeDB.Type]
public partial record ReturnEnum : SpacetimeDB.TaggedEnum<(
    uint A,
    string B
    )>;

[SpacetimeDB.Table(Name = "my_table", Public = true)]
public partial class MyTable
{
    public ReturnStruct Field { get; set; } = new(0, string.Empty);
}

public static partial class Module
{
    [SpacetimeDB.Table(Name = "ExampleData", Public = true)]
    public partial struct ExampleData
    {
        [SpacetimeDB.PrimaryKey]
        public uint Id;

        [SpacetimeDB.Index.BTree]
        public uint Indexed;
    }

    [SpacetimeDB.Table(Name = "Player", Public = true)]
    public partial struct Player
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;

        [SpacetimeDB.Unique]
        public Identity Identity;

        public string Name;
    }

    [SpacetimeDB.Table(Name = "PlayerLevel", Public = true)]
    public partial struct PlayerLevel
    {
        [SpacetimeDB.Unique]
        public ulong PlayerId;

        [SpacetimeDB.Index.BTree]
        public ulong Level;
    }

    [SpacetimeDB.Type]
    public partial struct PlayerAndLevel
    {
        public ulong Id;
        public Identity Identity;
        public string Name;
        public ulong Level;
    }

    // At-most-one row: return T?
    [SpacetimeDB.View(Name = "MyPlayer", Public = true)]
    public static Player? MyPlayer(ViewContext ctx)
    {
        return ctx.Db.Player.Identity.Find(ctx.Sender) as Player?;
    }

    // Multiple rows: return a list
    [SpacetimeDB.View(Name = "PlayersForLevel", Public = true)]
    public static List<PlayerAndLevel> PlayersForLevel(AnonymousViewContext ctx)
    {
        var rows = new List<PlayerAndLevel>();
        foreach (var player in ctx.Db.PlayerLevel.Level.Filter(1))
        {
            if (ctx.Db.Player.Id.Find(player.PlayerId) is Player p)
            {
                var row = new PlayerAndLevel
                {
                    Id = p.Id,
                    Identity = p.Identity,
                    Name = p.Name,
                    Level = player.Level
                };
                rows.Add(row);
            }
        }
        return rows;
    }

    [SpacetimeDB.Reducer]
    public static void Delete(ReducerContext ctx, uint id)
    {
        ctx.Db.ExampleData.Id.Delete(id);
    }

    [SpacetimeDB.Reducer]
    public static void Add(ReducerContext ctx, uint id, uint indexed)
    {
        ctx.Db.ExampleData.Insert(new ExampleData { Id = id, Indexed = indexed });
    }

    [SpacetimeDB.Reducer]
    public static void ThrowError(ReducerContext ctx, string error)
    {
        throw new Exception(error);
    }

    [Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        Log.Info($"Connect {ctx.Sender}");

        if (ctx.Db.Player.Identity.Find(ctx.Sender) is Player player)
        {
            // We are not logging player login status, so do nothing
        }
        else
        {
            // Lets setup a new player with a level of 1
            ctx.Db.Player.Insert(new Player { Identity = ctx.Sender, Name = "NewPlayer" });
            var playerId = (ctx.Db.Player.Identity.Find(ctx.Sender)!).Value.Id;
            ctx.Db.PlayerLevel.Insert(new PlayerLevel { PlayerId = playerId, Level = 1 });
        }
    }

    [SpacetimeDB.Procedure]
    public static uint ReturnPrimitive(ProcedureContext ctx, uint lhs, uint rhs)
    {
        return lhs + rhs;
    }

    [SpacetimeDB.Procedure]
    public static ReturnStruct ReturnStructProcedure(ProcedureContext ctx, uint a, string b)
    {
        return new ReturnStruct(a, b);
    }

    [SpacetimeDB.Procedure]
    public static ReturnEnum ReturnEnumA(ProcedureContext ctx, uint a)
    {
        return new ReturnEnum.A(a);
    }

    [SpacetimeDB.Procedure]
    public static ReturnEnum ReturnEnumB(ProcedureContext ctx, string b)
    {
        return new ReturnEnum.B(b);
    }

    [SpacetimeDB.Procedure]
    public static SpacetimeDB.Unit WillPanic(ProcedureContext ctx)
    {
        throw new InvalidOperationException("This procedure is expected to panic");
    }
}
