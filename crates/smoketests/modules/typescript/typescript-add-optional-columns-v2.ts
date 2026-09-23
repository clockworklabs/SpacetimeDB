import { schema, table, t } from "spacetimedb/server";

const AppUsers = table(
  { name: "users", public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    emailAddress: t.string().index("btree"),
    age: t.number().optional().default(undefined),
    isActive: t.bool().default(false).index(),
  },
);

const spacetimedb = schema({
  AppUsers,
});
export default spacetimedb;

export const find_user_by_email = spacetimedb.reducer(
  { emailAddress: t.string() },
  (ctx, { emailAddress }) => {
    let count = 0;
    for (const _row of ctx.db.AppUsers.emailAddress.filter(emailAddress)) {
      count += 1;
    }
    console.info(`matched ${count}`);
  },
);

export const find_users_by_active_status = spacetimedb.reducer(
  { isActive: t.bool() },
  (ctx, { isActive }) => {
    let count = 0;
    for (const _row of ctx.db.AppUsers.isActive.filter(isActive)) {
      count += 1;
    }
    console.info(`matched active users ${count}`);
  },
);
