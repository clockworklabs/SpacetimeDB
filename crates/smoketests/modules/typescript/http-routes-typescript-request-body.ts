import { Router, SyncResponse, schema } from "spacetimedb/server";

const spacetimedb = schema({});
export default spacetimedb;

export const reverse_bytes = spacetimedb.httpHandler((_ctx, req) => {
  const reversed = req.bytes();
  reversed.reverse();
  return new SyncResponse(reversed);
});

export const reverse_words = spacetimedb.httpHandler((_ctx, req) => {
  let body;
  try {
    body = new TextDecoder("utf-8", { fatal: true }).decode(req.bytes());
  } catch {
    return new SyncResponse("request body must be valid UTF-8", { status: 400 });
  }

  const reversed = body.split(" ").reverse().join(" ");
  return new SyncResponse(reversed);
});

export const router = spacetimedb.httpRouter(
  new Router()
    .post("/reverse-bytes", reverse_bytes)
    .post("/reverse-words", reverse_words)
);
