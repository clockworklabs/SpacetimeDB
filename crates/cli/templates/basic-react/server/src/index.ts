import { spacetimedb, t } from 'spacetimedb/server';

export {
  __call_reducer__,
  __describe_module__,
} from 'spacetimedb/server';

const personRow = {
  id: t.u32().primaryKey().autoInc(),
  name: t.string(),
};

const person = spacetimedb.table('person', personRow);

spacetimedb.reducer('add_person', { name: t.string() }, (ctx, { name }) => {
  ctx.db.person.insert({ id: 0, name });
});

spacetimedb.reducer('say_hello', {}, ctx => {
  for (const p of ctx.db.person.iter()) {
    console.info(`Hello, ${p.name}!`);
  }
});
