using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "CommandResult", Public = true)]
    public partial struct CommandResult
    {
        [PrimaryKey] public string RequestId;
        public bool Success;
        public string Message;
    }

    [Reducer]
    public static void RunCommand(ReducerContext ctx, string requestId, int value) =>
        ctx.Db.CommandResult.Insert(new CommandResult { RequestId = requestId, Success = true, Message = $"value={value}" });
}
