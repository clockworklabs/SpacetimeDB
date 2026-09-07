import { schema, table, t } from 'spacetimedb/server';

const app = schema({
  smokeItem: table({ public: true }, {
    id: t.u64().primaryKey(),
    value: t.string(),
  }),
});

export default app;

export const seed = app.reducer({ value: t.string() }, (ctx, { value }) => {
  ctx.db.smokeItem.insert({ id: 1n, value });
});
