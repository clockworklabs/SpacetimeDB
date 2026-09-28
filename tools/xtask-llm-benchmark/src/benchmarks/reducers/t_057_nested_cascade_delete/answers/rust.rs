use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = workspace, public)]
pub struct Workspace {
    #[primary_key]
    pub id: u64,
}
#[table(accessor = project, public)]
pub struct Project {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub workspace_id: u64,
}
#[table(accessor = task_item, public)]
pub struct TaskItem {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub project_id: u64,
}
#[table(accessor = task_note, public)]
pub struct TaskNote {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub task_id: u64,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    for id in [1, 2] {
        ctx.db.workspace().insert(Workspace { id });
        ctx.db.project().insert(Project { id, workspace_id: id });
        ctx.db.task_item().insert(TaskItem { id, project_id: id });
        ctx.db.task_note().insert(TaskNote { id, task_id: id });
    }
}

#[reducer]
pub fn delete_workspace(ctx: &ReducerContext, id: u64) {
    let projects: Vec<_> = ctx.db.project().workspace_id().filter(id).collect();
    for project in projects {
        let tasks: Vec<_> = ctx.db.task_item().project_id().filter(project.id).collect();
        for task in tasks {
            let notes: Vec<_> = ctx.db.task_note().task_id().filter(task.id).collect();
            for note in notes {
                ctx.db.task_note().id().delete(note.id);
            }
            ctx.db.task_item().id().delete(task.id);
        }
        ctx.db.project().id().delete(project.id);
    }
    ctx.db.workspace().id().delete(id);
}
