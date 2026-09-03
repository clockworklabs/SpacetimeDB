import { schema, table, t } from 'spacetimedb/server';

const fetchedRecord = table({ name: 'fetched_record', public: true }, {
  id: t.u64().primaryKey(), status: t.u16(), validBody: t.bool(),
});
const spacetimedb = schema({ fetchedRecord });
export default spacetimedb;

export const fetch_and_store = spacetimedb.procedure(
  { url: t.string() }, t.unit(),
  (ctx, { url }) => {
    const response = ctx.http.fetch(url);
    const status = response.status;
    const validBody = response.text().includes('Example Domain');
    ctx.withTx(tx => tx.db.fetchedRecord.insert({ id: 1n, status, validBody }));
    return {};
  }
);
