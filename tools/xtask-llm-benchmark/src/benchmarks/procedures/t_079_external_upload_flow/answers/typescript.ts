import { Router, SyncResponse, schema, table, t } from 'spacetimedb/server';

const uploadedAsset = table({ name: 'uploaded_asset', public: true }, {
  id: t.u64().primaryKey(), url: t.string(), size: t.u64(),
});
const spacetimedb = schema({ uploadedAsset });
export default spacetimedb;

export const upload = spacetimedb.httpHandler((_ctx, _request) =>
  new SyncResponse('https://files.local/object-1', { status: 201 })
);
export const routes = spacetimedb.httpRouter(new Router().post('/upload', upload));

export const upload_and_register = spacetimedb.procedure(
  { serverUrl: t.string(), data: t.array(t.u8()) }, t.string(),
  (ctx, { serverUrl, data }) => {
    const response = ctx.http.fetch(
      `${serverUrl.replace(/\/+$/, '')}/v1/database/${ctx.databaseIdentity}/route/upload`,
      { method: 'POST', headers: { 'content-type': 'application/octet-stream' }, body: new Uint8Array(data) }
    );
    const assetUrl = response.text();
    ctx.withTx(tx => tx.db.uploadedAsset.insert({ id: 1n, url: assetUrl, size: BigInt(data.length) }));
    return assetUrl;
  }
);
