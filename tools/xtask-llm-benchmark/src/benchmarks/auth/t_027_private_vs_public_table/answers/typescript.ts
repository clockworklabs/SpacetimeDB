import { schema, table, t } from 'spacetimedb/server';

const user_internal = table({
  name: 'user_internal',
}, {
  id: t.u64().primaryKey().autoInc(),
  name: t.string(),
  email: t.string(),
  passwordHash: t.string(),
});

const user_public = table({
  name: 'user_public',
  public: true,
}, {
  id: t.u64().primaryKey(),
  name: t.string(),
});

const spacetimedb = schema({ user_internal, user_public });
export default spacetimedb;

export const register_user = spacetimedb.reducer(
  { name: t.string(), email: t.string(), passwordHash: t.string() },
  (ctx, { name, email, passwordHash }) => {
    const internal = ctx.db.user_internal.insert({
      id: 0n,
      name,
      email,
      passwordHash,
    });
    ctx.db.user_public.insert({
      id: internal.id,
      name,
    });
  }
);
