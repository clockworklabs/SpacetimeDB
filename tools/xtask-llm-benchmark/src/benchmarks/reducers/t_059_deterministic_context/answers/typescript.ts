import { schema, table, t } from 'spacetimedb/server';

const generatedValue = table(
  { name: 'generated_value', public: true },
  { id: t.u64().primaryKey().autoInc(), createdAt: t.timestamp(), randomValue: t.i64() }
);
const spacetimedb = schema({ generatedValue });
export default spacetimedb;

export const generate = spacetimedb.reducer(ctx => {
  ctx.db.generatedValue.insert({
    id: 0n,
    createdAt: ctx.timestamp,
    randomValue: BigInt(ctx.random.integerInRange(1, Number.MAX_SAFE_INTEGER)),
  });
});
