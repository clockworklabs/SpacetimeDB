import { Router, SyncResponse, schema } from "spacetimedb/server";

const spacetimedb = schema({});
export default spacetimedb;

export const empty_root = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("empty")
);

export const slash_root = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("slash")
);

export const foo = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("foo")
);

export const foo_slash = spacetimedb.httpHandler((_ctx, _req) =>
  new SyncResponse("foo-slash")
);

export const router = spacetimedb.httpRouter(
  new Router()
    .get("", empty_root)
    .get("/", slash_root)
    .get("/foo", foo)
    .get("/foo/", foo_slash)
);
