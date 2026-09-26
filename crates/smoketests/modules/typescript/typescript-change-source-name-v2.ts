import { schema, table, t } from "spacetimedb/server";

const renamedUsers = table(
  { name: "users", public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    emailAddress: t.string().index("btree"),
  },
);

const spacetimedb = schema({
  renamedUsers,
});
export default spacetimedb;

export const find_user_by_email = spacetimedb.reducer(
  { emailAddress: t.string() },
  (ctx, { emailAddress }) => {
    let count = 0;
    for (const _row of ctx.db.renamedUsers.emailAddress.filter(emailAddress)) {
      count += 1;
    }
    console.info(`matched ${count}`);
  },
);
