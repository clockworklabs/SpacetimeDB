import { schema, t } from 'spacetimedb/server';

const FetchSummary = t.object('FetchSummary', {
  status: t.u16(), jsonContentType: t.bool(), hasTables: t.bool(),
});
const spacetimedb = schema({});
export default spacetimedb;

export const fetch_schema_summary = spacetimedb.procedure(
  { serverUrl: t.string() }, FetchSummary,
  (ctx, { serverUrl }) => {
    const response = ctx.http.fetch(`${serverUrl.replace(/\/+$/, '')}/v1/database/${ctx.databaseIdentity}/schema?version=9`);
    return {
      status: response.status,
      jsonContentType: (response.headers.get('content-type') ?? '').includes('application/json'),
      hasTables: response.text().includes('"tables"'),
    };
  }
);
