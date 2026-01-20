using SpacetimeDB;

public static partial class Module
{
    [Table(Name = "Result")]
    public partial struct Result
    {
        [PrimaryKey] public int Id;
        public int Sum;
    }

    static int Add(int a, int b) => a + b;

    [Reducer]
    public static void ComputeSum(ReducerContext ctx, int id, int a, int b)
    {
        ctx.Db.Result.Insert(new Result { Id = id, Sum = Add(a, b) });
    }
}
