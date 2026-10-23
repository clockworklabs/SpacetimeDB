import { Router, SyncResponse, schema } from 'spacetimedb/server';

const spacetimedb = schema({});
export default spacetimedb;

export const listItems = spacetimedb.httpHandler((_ctx, _request) => new SyncResponse('list'));
export const createItem = spacetimedb.httpHandler((_ctx, request) =>
  new SyncResponse(`created:${request.text()}`, { status: 201 })
);
export const routes = spacetimedb.httpRouter(
  new Router().get('/items', listItems).post('/items', createItem)
);
