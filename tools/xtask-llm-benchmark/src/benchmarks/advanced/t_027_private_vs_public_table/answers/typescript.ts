import { schema, table, t } from 'spacetimedb/server';

const userInternal = table({
  name: 'user_internal',
}, {
  id: t.u64().primaryKey().autoInc(),
  name: t.string(),
  email: t.string(),
  passwordHash: t.string(),
});

const userPublic = table({
  name: 'user_public',
  public: true,
}, {
  id: t.u64().primaryKey(),
  name: t.string(),
});

const spacetimedb = schema({ userInternal, userPublic });
export default spacetimedb;

export const register_user = spacetimedb.reducer(
  { name: t.string(), email: t.string(), passwordHash: t.string() },
  (ctx, { name, email, passwordHash }) => {
    const internal = ctx.db.userInternal.insert({
      id: 0n,
      name,
      email,
      passwordHash,
    });
    ctx.db.userPublic.insert({
      id: internal.id,
      name,
    });
  }
);
