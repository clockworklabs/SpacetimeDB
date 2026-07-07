import { schema, table, t } from 'spacetimedb/server';

const task = table({
  name: 'task',
}, {
  id: t.u64().primaryKey().autoInc(),
  title: t.string(),
  completed: t.bool(),
});

const firstIncomplete = table({
  name: 'first_incomplete',
}, {
  taskId: t.u64().primaryKey(),
  title: t.string(),
});

const spacetimedb = schema({ task, firstIncomplete });
export default spacetimedb;

export const find_first_incomplete = spacetimedb.reducer(
  (ctx) => {
    for (const t of ctx.db.task.iter()) {
      if (!t.completed) {
        ctx.db.firstIncomplete.insert({
          taskId: t.id,
          title: t.title,
        });
        return;
      }
    }
  }
);
