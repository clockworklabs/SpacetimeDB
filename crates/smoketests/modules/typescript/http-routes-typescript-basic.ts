import { Router, SyncResponse, schema, table, t } from "spacetimedb/server";

const entry = table(
  { name: "entry", public: true },
  {
    id: t.u64().primaryKey(),
    value: t.string(),
  }
);

const spacetimedb = schema({ entry });
export default spacetimedb;

export const get_simple = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("ok")
);

export const post_insert = spacetimedb.httpHandler((ctx, _req) => {
  ctx.withTx(tx => {
    const id = BigInt(tx.db.entry.count());
    tx.db.entry.insert({ id, value: "posted" });
  });
  return new SyncResponse("inserted");
});

export const get_count = spacetimedb.httpHandler((ctx, _req) => {
  const count = ctx.withTx(tx => tx.db.entry.count());
  return new SyncResponse(String(count));
});

export const any_handler = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("any")
);

export const header_echo = spacetimedb.httpHandler((_ctx, req) =>
  new SyncResponse(req.headers.get("x-echo") ?? "")
);

export const set_response_header = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("header-set", { headers: { "x-response": "set" } })
);

export const body_handler = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("non-empty")
);

export const teapot = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("teapot", { status: 418 })
);

export const router = spacetimedb.httpRouter(
  new Router()
    .get("/get", get_simple)
    .post("/post", post_insert)
    .get("/count", get_count)
    .any("/any", any_handler)
    .get("/header", header_echo)
    .get("/set-header", set_response_header)
    .get("/body", body_handler)
    .get("/teapot", teapot)
);
