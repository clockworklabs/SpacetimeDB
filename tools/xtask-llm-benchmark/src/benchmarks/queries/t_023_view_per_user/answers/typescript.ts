import { schema, table, t } from 'spacetimedb/server';

const profile = table({
  name: 'profile',
  public: true,
}, {
  id: t.u64().primaryKey().autoInc(),
  identity: t.identity().unique(),
  name: t.string(),
  bio: t.string(),
});

const spacetimedb = schema({ profile });
export default spacetimedb;

export const my_profile = spacetimedb.view(
  { name: 'my_profile', public: true },
  t.option(profile.rowType),
  (ctx) => {
    return ctx.db.profile.identity.find(ctx.sender) ?? undefined;
  }
);
