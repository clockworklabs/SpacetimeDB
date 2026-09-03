import { schema, t } from 'spacetimedb/server';

const FetchSummary = t.object('FetchSummary', {
  status: t.u16(), htmlContentType: t.bool(), hasExampleDomain: t.bool(),
});
const spacetimedb = schema({});
export default spacetimedb;

export const fetch_page_summary = spacetimedb.procedure(
  { url: t.string() }, FetchSummary,
  (ctx, { url }) => {
    const response = ctx.http.fetch(url);
    return {
      status: response.status,
      htmlContentType: (response.headers.get('content-type') ?? '').includes('text/html'),
      hasExampleDomain: response.text().includes('Example Domain'),
    };
  }
);
