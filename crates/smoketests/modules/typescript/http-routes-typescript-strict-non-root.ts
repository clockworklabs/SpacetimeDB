import { Router, SyncResponse, schema } from "spacetimedb/server";

const spacetimedb = schema({});
export default spacetimedb;

export const foo = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("foo")
);

export const foo_slash = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("foo-slash")
);

export const router = spacetimedb.httpRouter(
  new Router()
    .get("/foo", foo)
    .get("/foo/", foo_slash)
);
