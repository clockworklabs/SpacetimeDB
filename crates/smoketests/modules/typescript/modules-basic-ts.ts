import { schema, t, table } from "spacetimedb/server";

const person = table(
    { name: "person", public: true },
    {
        id: t.u64().primaryKey().autoInc(),
        name: t.string()
    }
);
const spacetimedb = schema({ person });
export default spacetimedb;

export const add = spacetimedb.reducer({ name: t.string() }, (ctx, { name }) => {
  ctx.db.person.insert({ id: 0n, name });
});
