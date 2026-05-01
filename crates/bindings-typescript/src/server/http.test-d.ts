import { table } from '../lib/table';
import t from '../lib/type_builders';
import {
  type HandlerContext,
  Request,
  Response,
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

  return new Response('hello', {
    headers: { 'content-type': 'text/plain' },
    status: 200,
  });
});

const _typedHello: (ctx: HandlerContext<any>, req: Request) => Response = (
  ctx,
  req
) => {
  void ctx.timestamp;
  return new Response(req.text());
};

const routes = stdb.httpRouter(
  Router.new()
    .get('/hello', hello)
    .post('/hello-post', hello)
    .nest('/api', Router.new().any('/v1', hello))
    .merge(Router.new().get('', hello))
);

void routes;

// @ts-expect-error handlers must return Response
stdb.httpHandler((_ctx, _req) => 123);

// @ts-expect-error handlers must take a Request as the second argument
stdb.httpHandler((_ctx, _req: number) => new Response('bad'));
