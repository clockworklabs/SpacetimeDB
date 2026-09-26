import { Router, SyncResponse, schema, table, t } from "spacetimedb/server";

const data = table(
  { name: "data" },
  {
    id: t.u64().primaryKey().autoInc(),
    body: t.array(t.u8()),
  }
);

const spacetimedb = schema({ data });
export default spacetimedb;

export const insert = spacetimedb.httpHandler((ctx, req) => {
  const body = Array.from(req.bytes());
  const id = ctx.withTx(tx => tx.db.data.insert({ id: 0n, body }).id);
  return new SyncResponse(String(id));
});

export const retrieve = spacetimedb.httpHandler((ctx, req) => {
  const query = req.uri.split("?", 2)[1] ?? "";
  const idText = query.startsWith("id=") ? query.slice(3) : "";
  const id = BigInt(idText);
  const body = ctx.withTx(tx => tx.db.data.id.find(id)?.body);
  if (body != null) {
    return new SyncResponse(new Uint8Array(body));
  }
  return new SyncResponse(null, { status: 404 });
});

export const router = spacetimedb.httpRouter(
  new Router().post("/insert", insert).get("/retrieve", retrieve)
);
