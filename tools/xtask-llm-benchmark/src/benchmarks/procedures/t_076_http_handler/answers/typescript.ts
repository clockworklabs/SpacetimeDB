import { Router, SyncResponse, schema } from 'spacetimedb/server';

const spacetimedb = schema({});
export default spacetimedb;

export const echo = spacetimedb.httpHandler((_ctx, request) =>
  new SyncResponse(`echo:${request.text()}`, { status: 201, headers: { 'content-type': 'text/plain' } })
);
export const routes = spacetimedb.httpRouter(new Router().post('/echo', echo));
