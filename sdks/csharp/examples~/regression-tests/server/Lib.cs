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
    [SpacetimeDB.Table(Name = "example_data", Public = true)]
    public partial struct ExampleData
    {
        [SpacetimeDB.PrimaryKey]
        public uint Id;

        [SpacetimeDB.Index.BTree]
        public uint Indexed;
    }

    [SpacetimeDB.Table(Name = "player", Public = true)]
    public partial struct Player
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;

        [SpacetimeDB.Unique]
        public Identity Identity;

        public string Name;
    }

    [SpacetimeDB.Table(Name = "player_level", Public = true)]
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
    [SpacetimeDB.View(Name = "my_player", Public = true)]
    public static Player? MyPlayer(ViewContext ctx)
    {
        return ctx.Db.player.Identity.Find(ctx.Sender) as Player?;
    }

    // Multiple rows: return a list
    [SpacetimeDB.View(Name = "players_for_level", Public = true)]
    public static List<PlayerAndLevel> PlayersForLevel(AnonymousViewContext ctx)
    {
        var rows = new List<PlayerAndLevel>();
        foreach (var player in ctx.Db.player_level.Level.Filter(1))
        {
            if (ctx.Db.player.Id.Find(player.PlayerId) is Player p)
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
        LogStopwatch sw = new("Delete");
        ctx.Db.example_data.Id.Delete(id);
    }

    [SpacetimeDB.Reducer]
    public static void Add(ReducerContext ctx, uint id, uint indexed)
    {
        ctx.Db.example_data.Insert(new ExampleData { Id = id, Indexed = indexed });
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

        if (ctx.Db.player.Identity.Find(ctx.Sender) is Player player)
        {
            // We are not logging player login status, so do nothing
        }
        else
        {
            // Lets setup a new player with a level of 1
            ctx.Db.player.Insert(new Player { Identity = ctx.Sender, Name = "NewPlayer" });
            var playerId = (ctx.Db.player.Identity.Find(ctx.Sender)!).Value.Id;
            ctx.Db.player_level.Insert(new PlayerLevel { PlayerId = playerId, Level = 1 });
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
    
#pragma warning disable STDB_UNSTABLE
    [SpacetimeDB.Procedure]
    public static void InsertWithTxCommit(ProcedureContext ctx)
    {
        ctx.WithTx(tx =>
        {
            tx.Db.my_table.Insert(new MyTable
            {
                Field = new ReturnStruct(a: 42, b: "magic")
            });
            return 0; // discard result
        });

        AssertRowCount(ctx, 1);
    }

    [SpacetimeDB.Procedure]
    public static void InsertWithTxRollback(ProcedureContext ctx)
    {
        var _ = ctx.TryWithTx<SpacetimeDB.Unit, InvalidOperationException>(tx =>
        {
            tx.Db.my_table.Insert(new MyTable
            {
                Field = new ReturnStruct(a: 42, b: "magic")
            });

            return SpacetimeDB.ProcedureContext.TxResult<SpacetimeDB.Unit, InvalidOperationException>.Failure(
                new InvalidOperationException("rollback"));
        });

        AssertRowCount(ctx, 0);
    }

    private static void AssertRowCount(ProcedureContext ctx, ulong expected)
    {
        ctx.WithTx(tx =>
        {
            ulong actual = tx.Db.my_table.Count;
            if (actual != expected)
            {
                throw new InvalidOperationException(
                    $"Expected {expected} MyTable rows but found {actual}."
                );
            }
            return 0;
        });
    }
#pragma warning restore STDB_UNSTABLE
}
