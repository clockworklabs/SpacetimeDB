using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Log")]
    [SpacetimeDB.Index.BTree(Accessor = "by_user_day", Columns = new[] { nameof(UserId), nameof(Day) })]
    public partial struct Log
    {
        [PrimaryKey, AutoInc] public ulong Id;
        public int UserId;
        public int Day;
        public string Message;
    }

    [Reducer]
    public static void Seed(ReducerContext ctx)
    {
        ctx.Db.Log.Insert(new Log { Id = 0, UserId = 7, Day = 1, Message = "a" });
        ctx.Db.Log.Insert(new Log { Id = 0, UserId = 7, Day = 2, Message = "b" });
        ctx.Db.Log.Insert(new Log { Id = 0, UserId = 9, Day = 1, Message = "c" });
    }
}
