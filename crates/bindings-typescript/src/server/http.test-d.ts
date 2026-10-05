import { table } from '../lib/table';
import t from '../lib/type_builders';
import {
  type HandlerContext,
  Request,
  SyncResponse,
  Router,
  schema,
} from './index';

const person = table(
  {},
  {
    id: t.u32().primaryKey(),
    name: t.string(),
  }
);

const stdb = schema({ person });

const hello = stdb.httpHandler((ctx, req) => {
  void ctx.identity;
  void ctx.random;
  req.text();
  req.json();

  ctx.withTx(tx => {
    tx.db.person.insert({ id: 1, name: 'alice' });
  });

  return new SyncResponse('hello', {
    headers: { 'content-type': 'text/plain' },
    status: 200,
  });
});

const _typedHello: (ctx: HandlerContext<any>, req: Request) => SyncResponse = (
  ctx,
  req
) => {
  void ctx.timestamp;
  return new SyncResponse(req.text());
};

const named = stdb.httpHandler({ name: 'hello' }, (_ctx, _req) => {
  return new SyncResponse('named');
});

const routes = stdb.httpRouter(
  new Router()
    .get('/hello', hello)
    .get('/named', named)
    .post('/hello-post', hello)
    .nest('/api', new Router().any('/v1', hello))
    .merge(new Router().get('', hello))
);

void routes;

// @ts-expect-error handlers must return SyncResponse
stdb.httpHandler((_ctx, _req) => 123);

// @ts-expect-error handlers must take HandlerContext as the first argument
stdb.httpHandler((_ctx: number, _req: Request) => new SyncResponse('bad'));

// @ts-expect-error handlers must take a Request as the second argument
stdb.httpHandler((_ctx, _req: number) => new SyncResponse('bad'));

stdb.httpHandler((ctx, req) => {
  // @ts-expect-error HTTP handlers do not expose sender directly
  void ctx.sender;
  // @ts-expect-error HTTP handlers do not expose connectionId directly
  void ctx.connectionId;
  // @ts-expect-error HTTP handlers do not expose db directly
  void ctx.db;
  return new SyncResponse(req.text());
});

// @ts-expect-error routers must reference exported http handlers, not raw functions
new Router().get('/raw', (_ctx, _req) => new SyncResponse('bad'));
