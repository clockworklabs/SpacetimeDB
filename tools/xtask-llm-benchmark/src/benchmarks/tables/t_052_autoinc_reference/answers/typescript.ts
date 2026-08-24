import { schema, table, t } from 'spacetimedb/server';

const parent = table(
  { name: 'parent', public: true },
  { id: t.u64().primaryKey().autoInc(), name: t.string() }
);

const child = table(
  {
    name: 'child',
    public: true,
    indexes: [{ accessor: 'byParent', algorithm: 'btree', columns: ['parentId'] }],
  },
  { id: t.u64().primaryKey().autoInc(), parentId: t.u64(), name: t.string() }
);

const spacetimedb = schema({ parent, child });
export default spacetimedb;

export const create_family = spacetimedb.reducer(
  { parentName: t.string(), childNames: t.array(t.string()) },
  (ctx, { parentName, childNames }) => {
    const insertedParent = ctx.db.parent.insert({ id: 0n, name: parentName });
    for (const name of childNames) {
      ctx.db.child.insert({ id: 0n, parentId: insertedParent.id, name });
    }
  }
);
