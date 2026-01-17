using SpacetimeDB;

public static partial class Module
{
    [Type]
    public partial struct Score
    {
        public int Left;
        public int Right;
    }
  
    [Table(Name = "Result", Public = true)]
    public partial struct Result
    {
        [PrimaryKey] public int Id;
        public Score Value;
    }

    [Reducer]
    public static void SetScore(ReducerContext ctx, int id, int left, int right)
    {
        ctx.Db.Result.Insert(new Result { Id = id, Value = new Score { Left = left, Right = right } });
    }
}
