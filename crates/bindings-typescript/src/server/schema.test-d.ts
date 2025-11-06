import { schema } from './schema';
import { table } from './table';
import t from './type_builders';

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
        name: 'name_idx',
        algorithm: 'btree',
        columns: ['name'] as const,
      },
    ],
  },
  {
    id: t.u32().primaryKey(),
    name: t.string(),
    married: t.bool(),
    id2: t.identity(),
    age: t.u32(),
    age2: t.u16(),
  }
);

const spacetimedb = schema(person);

spacetimedb.init(ctx => {
  ctx.db.person.id_name_idx.filter(1);
  ctx.db.person.id_name_idx.filter([1, "aname"]);
});
