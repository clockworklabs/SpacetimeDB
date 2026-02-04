import { schema } from './schema';
import { table } from '../lib/table';
import t from '../lib/type_builders';

const person = table(
  {
    name: 'person',
    indexes: [
      {
        name: 'id_name_idx',
        algorithm: 'btree',
        columns: ['id', 'name'] as const,
      },
      {
        name: 'id_name2_idx',
        algorithm: 'btree',
        columns: ['id', 'name2'] as const,
      },
      {
        name: 'name_idx',
        algorithm: 'btree',
        columns: ['name'] as const,
      },
    ],
  },
  {
    id: t.u32().primaryKey(),
    name: t.string(),
    name2: t.string().unique(),
    married: t.bool(),
    id2: t.identity(),
    age: t.u32(),
    age2: t.u16(),
  }
);

const spacetimedb = schema(person);

spacetimedb.init(ctx => {
  ctx.db.person.id_name_idx.filter(1);
  ctx.db.person.id_name_idx.filter([1, 'aname']);
  // ctx.db.person.id_name2_idx.find

  // @ts-expect-error id2 is not indexed, so this should not exist at all.
  const _id2 = ctx.db.person.id2;

  ctx.db.person.id.find(2);
});
