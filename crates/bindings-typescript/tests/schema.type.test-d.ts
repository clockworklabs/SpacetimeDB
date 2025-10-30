import { schema, table, t } from '../src/server';

const person = table({ name: 'person' }, {
  id: t.u32(),
  name: t.string(),
});

const spacetime = schema(person);

spacetime.reducer('test', (ctx) => {
  for (const row of ctx.db.person.iter()) {
    row.name
  }
  ctx.db.person.insert({ id: 1, name: 'ok' });
  // @ts-expect-error
  ctx.db.person.insert({ missing: 'nope' });
});

