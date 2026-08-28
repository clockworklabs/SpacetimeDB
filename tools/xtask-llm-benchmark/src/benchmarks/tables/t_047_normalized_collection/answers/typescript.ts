import { schema, table, t } from 'spacetimedb/server';

const collectionOwner = table(
  { name: 'collection_owner', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
  }
);

const childItem = table(
  {
    name: 'child_item',
    public: true,
    indexes: [{ accessor: 'byOwner', algorithm: 'btree', columns: ['ownerId'] }],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    ownerId: t.u64(),
    value: t.string(),
  }
);

export default schema({ collectionOwner, childItem });
