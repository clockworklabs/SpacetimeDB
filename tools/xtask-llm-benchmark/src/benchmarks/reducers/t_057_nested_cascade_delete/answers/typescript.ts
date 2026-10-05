import { schema, table, t } from 'spacetimedb/server';

const workspace = table({ name: 'workspace', public: true }, { id: t.u64().primaryKey() });
const project = table({ name: 'project', public: true }, { id: t.u64().primaryKey(), workspaceId: t.u64().index('btree') });
const taskItem = table({ name: 'task_item', public: true }, { id: t.u64().primaryKey(), projectId: t.u64().index('btree') });
const taskNote = table({ name: 'task_note', public: true }, { id: t.u64().primaryKey(), taskId: t.u64().index('btree') });
const spacetimedb = schema({ workspace, project, taskItem, taskNote });
export default spacetimedb;

export const seed = spacetimedb.reducer(ctx => {
  for (const id of [1n, 2n]) {
    ctx.db.workspace.insert({ id });
    ctx.db.project.insert({ id, workspaceId: id });
    ctx.db.taskItem.insert({ id, projectId: id });
    ctx.db.taskNote.insert({ id, taskId: id });
  }
});

export const delete_workspace = spacetimedb.reducer({ id: t.u64() }, (ctx, { id }) => {
  for (const project of [...ctx.db.project.workspaceId.filter(id)]) {
    for (const task of [...ctx.db.taskItem.projectId.filter(project.id)]) {
      for (const note of [...ctx.db.taskNote.taskId.filter(task.id)]) ctx.db.taskNote.id.delete(note.id);
      ctx.db.taskItem.id.delete(task.id);
    }
    ctx.db.project.id.delete(project.id);
  }
  ctx.db.workspace.id.delete(id);
});
