import { Router, SyncResponse, schema, table, t } from 'spacetimedb/server';

const uploadedAsset = table({ name: 'uploaded_asset', public: true }, {
  id: t.u64().primaryKey(), url: t.string(), size: t.u64(), status: t.u16(), responseBodyPresent: t.bool(),
});
const spacetimedb = schema({ uploadedAsset });
export default spacetimedb;

export const upload = spacetimedb.httpHandler((_ctx, _request) =>
  new SyncResponse('https://files.local/object-1', { status: 201 })
);
export const routes = spacetimedb.httpRouter(new Router().post('/upload', upload));

export const upload_and_register = spacetimedb.procedure(
  { uploadUrl: t.string(), data: t.array(t.u8()) }, t.string(),
  (ctx, { uploadUrl, data }) => {
    const response = ctx.http.fetch(
      uploadUrl,
      { method: 'POST', headers: { 'content-type': 'application/octet-stream' }, body: new Uint8Array(data) }
    );
    if (response.status < 200 || response.status >= 300) throw new Error(`upload failed: ${response.status}`);
    const responseBodyPresent = response.bytes().length > 0;
    ctx.withTx(tx => tx.db.uploadedAsset.insert({
      id: 1n, url: uploadUrl, size: BigInt(data.length), status: response.status, responseBodyPresent,
    }));
    return uploadUrl;
  }
);
