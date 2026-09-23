import { Router, SyncResponse, schema } from "spacetimedb/server";

const spacetimedb = schema({});
export default spacetimedb;

export const echo_uri = spacetimedb.httpHandler((_ctx, req) =>
  new SyncResponse(req.uri)
);

export const router = spacetimedb.httpRouter(
  new Router().get("/echo-uri", echo_uri)
);
