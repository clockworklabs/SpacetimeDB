import { schema, table, t } from 'spacetimedb/server';

const config = table({
  name: 'config',
}, {
  id: t.u32().primaryKey(),
  admin: t.identity(),
});

const admin_log = table({
  name: 'admin_log',
  public: true,
}, {
  id: t.u64().primaryKey().autoInc(),
  action: t.string(),
});

const spacetimedb = schema({ config, admin_log });
export default spacetimedb;

export const bootstrap_admin = spacetimedb.reducer(
  {},
  (ctx) => {
    if (ctx.db.config.id.find(0)) {
      throw new Error('already bootstrapped');
    }
    ctx.db.config.insert({ id: 0, admin: ctx.sender });
  }
);

export const admin_action = spacetimedb.reducer(
  { action: t.string() },
  (ctx, { action }) => {
    const config = ctx.db.config.id.find(0);
    if (!config) throw new Error('not bootstrapped');
    if (!config.admin.equals(ctx.sender)) throw new Error('not admin');
    ctx.db.admin_log.insert({ id: 0n, action });
  }
);
