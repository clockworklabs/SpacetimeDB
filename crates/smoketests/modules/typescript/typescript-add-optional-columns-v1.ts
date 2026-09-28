import { schema, table, t } from "spacetimedb/server";

const AppUsers = table(
  { name: "users", public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    emailAddress: t.string().index("btree"),
  },
);

const spacetimedb = schema({
  AppUsers,
});
export default spacetimedb;

export const insert_user = spacetimedb.reducer(
  {
    name: t.string(),
    emailAddress: t.string(),
  },
  (ctx, { name, emailAddress }) => {
    ctx.db.AppUsers.insert({
      id: 0n,
      name,
      emailAddress,
    });
  },
);
