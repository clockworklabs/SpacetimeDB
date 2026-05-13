using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Task")]
    public partial struct Task
    {
        [PrimaryKey]
        [AutoInc]
        public ulong Id;
        public string Title;
        public bool Completed;
    }

    [Table(Accessor = "FirstIncomplete")]
    public partial struct FirstIncomplete
    {
        [PrimaryKey]
        public ulong TaskId;
        public string Title;
    }

    [Reducer]
    public static void FindFirstIncomplete(ReducerContext ctx)
    {
        foreach (var t in ctx.Db.Task.Iter())
        {
            if (!t.Completed)
            {
                ctx.Db.FirstIncomplete.Insert(new FirstIncomplete
                {
                    TaskId = t.Id,
                    Title = t.Title,
                });
                return;
            }
        }
    }
}
