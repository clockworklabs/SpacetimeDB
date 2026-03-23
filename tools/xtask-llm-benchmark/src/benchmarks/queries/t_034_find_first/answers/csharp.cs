using SpacetimeDB;
using System.Linq;

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
        var first = ctx.Db.Task.Iter().FirstOrDefault(t => !t.Completed);
        if (first.Title != null)
        {
            ctx.Db.FirstIncomplete.Insert(new FirstIncomplete
            {
                TaskId = first.Id,
                Title = first.Title,
            });
        }
    }
}
