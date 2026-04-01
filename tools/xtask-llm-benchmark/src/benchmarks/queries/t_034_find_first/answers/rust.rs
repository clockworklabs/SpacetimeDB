use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = task)]
pub struct Task {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub title: String,
    pub completed: bool,
}

#[table(accessor = first_incomplete)]
pub struct FirstIncomplete {
    #[primary_key]
    pub task_id: u64,
    pub title: String,
}

#[reducer]
pub fn find_first_incomplete(ctx: &ReducerContext) {
    if let Some(t) = ctx.db.task().iter().find(|t| !t.completed) {
        ctx.db.first_incomplete().insert(FirstIncomplete {
            task_id: t.id,
            title: t.title,
        });
    }
}
