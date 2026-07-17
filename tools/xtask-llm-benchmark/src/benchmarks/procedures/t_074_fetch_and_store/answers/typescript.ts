import { schema, table, t } from 'spacetimedb/server';

const fetchedRecord = table({ name: 'fetched_record', public: true }, {
  id: t.u64().primaryKey(), status: t.u16(), validSchema: t.bool(),
});
const spacetimedb = schema({ fetchedRecord });
export default spacetimedb;

export const fetch_and_store = spacetimedb.procedure(
  { serverUrl: t.string() }, t.unit(),
  (ctx, { serverUrl }) => {
    const response = ctx.http.fetch(`${serverUrl.replace(/\/+$/, '')}/v1/database/${ctx.databaseIdentity}/schema?version=9`);
    const status = response.status;
    const validSchema = response.text().includes('"tables"');
    ctx.withTx(tx => tx.db.fetchedRecord.insert({ id: 1n, status, validSchema }));
    return {};
  }
);
