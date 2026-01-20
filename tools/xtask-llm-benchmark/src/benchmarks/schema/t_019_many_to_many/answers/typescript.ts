import { table, schema, t } from 'spacetimedb/server';

export const User = table({
  name: 'user',
}, {
  userId: t.i32().primaryKey(),
  name: t.string(),
});

export const Group = table({
  name: 'group',
}, {
  groupId: t.i32().primaryKey(),
  title: t.string(),
});

export const Membership = table({
  name: 'membership',
  indexes: [
    { name: 'byUser', algorithm: 'btree', columns: ['userId'] },
    { name: 'byGroup', algorithm: 'btree', columns: ['groupId'] },
  ],
}, {
  id: t.i32().primaryKey(),
  userId: t.i32(),
  groupId: t.i32(),
});

const spacetimedb = schema(User, Group, Membership);

spacetimedb.reducer('seed', {},
  ctx => {
    ctx.db.user.insert({ userId: 1, name: "Alice" });
    ctx.db.user.insert({ userId: 2, name: "Bob" });

    ctx.db.group.insert({ groupId: 10, title: "Admin" });
    ctx.db.group.insert({ groupId: 20, title: "Dev" });

    ctx.db.membership.insert({ id: 1, userId: 1, groupId: 10 });
    ctx.db.membership.insert({ id: 2, userId: 1, groupId: 20 });
    ctx.db.membership.insert({ id: 3, userId: 2, groupId: 20 });
  }
);
