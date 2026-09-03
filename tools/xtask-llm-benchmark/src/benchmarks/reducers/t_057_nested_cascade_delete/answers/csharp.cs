using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Workspace", Public = true)] public partial struct Workspace { [PrimaryKey] public ulong Id; }
    [Table(Accessor = "Project", Public = true)] public partial struct Project { [PrimaryKey] public ulong Id; [SpacetimeDB.Index.BTree] public ulong WorkspaceId; }
    [Table(Accessor = "TaskItem", Public = true)] public partial struct TaskItem { [PrimaryKey] public ulong Id; [SpacetimeDB.Index.BTree] public ulong ProjectId; }
    [Table(Accessor = "TaskNote", Public = true)] public partial struct TaskNote { [PrimaryKey] public ulong Id; [SpacetimeDB.Index.BTree] public ulong TaskId; }

    [Reducer]
    public static void Seed(ReducerContext ctx)
    {
        foreach (ulong id in new ulong[] { 1, 2 })
        {
            ctx.Db.Workspace.Insert(new Workspace { Id = id });
            ctx.Db.Project.Insert(new Project { Id = id, WorkspaceId = id });
            ctx.Db.TaskItem.Insert(new TaskItem { Id = id, ProjectId = id });
            ctx.Db.TaskNote.Insert(new TaskNote { Id = id, TaskId = id });
        }
    }

    [Reducer]
    public static void DeleteWorkspace(ReducerContext ctx, ulong id)
    {
        foreach (var project in ctx.Db.Project.WorkspaceId.Filter(id).ToList())
        {
            foreach (var task in ctx.Db.TaskItem.ProjectId.Filter(project.Id).ToList())
            {
                foreach (var note in ctx.Db.TaskNote.TaskId.Filter(task.Id).ToList()) ctx.Db.TaskNote.Id.Delete(note.Id);
                ctx.Db.TaskItem.Id.Delete(task.Id);
            }
            ctx.Db.Project.Id.Delete(project.Id);
        }
        ctx.Db.Workspace.Id.Delete(id);
    }
}
