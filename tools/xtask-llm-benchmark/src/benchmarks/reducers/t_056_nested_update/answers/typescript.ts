import { schema, table, t } from 'spacetimedb/server';

const Preferences = t.object('Preferences', {
  theme: t.string(),
  emailNotifications: t.bool(),
  timezone: t.string(),
});
const profile = table(
  { name: 'profile', public: true },
  { id: t.u64().primaryKey(), preferences: Preferences }
);
const spacetimedb = schema({ profile });
export default spacetimedb;

export const create_profile = spacetimedb.reducer(
  { id: t.u64(), theme: t.string(), emailNotifications: t.bool(), timezone: t.string() },
  (ctx, args) => ctx.db.profile.insert({ id: args.id, preferences: {
    theme: args.theme, emailNotifications: args.emailNotifications, timezone: args.timezone,
  } })
);

export const update_theme = spacetimedb.reducer(
  { id: t.u64(), theme: t.string() },
  (ctx, { id, theme }) => {
    const found = ctx.db.profile.id.find(id);
    if (!found) throw new Error('profile not found');
    ctx.db.profile.id.update({
      ...found,
      preferences: { ...found.preferences, theme },
    });
  }
);
