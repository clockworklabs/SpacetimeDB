import { table, schema, t } from 'spacetimedb/server';

const user = table(
  {
    name: 'user',
  },
  {
    userId: t.u64().primaryKey().autoInc(),
    name: t.string(),
  }
);

const group = table(
  {
    name: 'group',
  },
  {
    groupId: t.u64().primaryKey().autoInc(),
    title: t.string(),
  }
);

const membership = table(
  {
    name: 'membership',
    indexes: [
      { name: 'byUser', algorithm: 'btree', columns: ['userId'] },
      { name: 'byGroup', algorithm: 'btree', columns: ['groupId'] },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    userId: t.u64(),
    groupId: t.u64(),
  }
);

const spacetimedb = schema({ user, group, membership });
export default spacetimedb;

export const seed = spacetimedb.reducer(ctx => {
  ctx.db.user.insert({ userId: 0n, name: 'Alice' });
  ctx.db.user.insert({ userId: 0n, name: 'Bob' });

  ctx.db.group.insert({ groupId: 0n, title: 'Admin' });
  ctx.db.group.insert({ groupId: 0n, title: 'Dev' });

  ctx.db.membership.insert({ id: 0n, userId: 1n, groupId: 1n });
  ctx.db.membership.insert({ id: 0n, userId: 1n, groupId: 2n });
  ctx.db.membership.insert({ id: 0n, userId: 2n, groupId: 2n });
});
