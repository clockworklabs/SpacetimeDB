import {
  HttpRequest,
  HttpResponse,
  type MethodOrAny,
  type HttpMethod,
} from '../lib/autogen/types';
import BinaryReader from '../lib/binary_reader';
import BinaryWriter from '../lib/binary_writer';
import { bsatnBaseSize } from '../lib/util';
import {
  Headers,
  SyncResponse,
  deserializeHeaders,
  serializeHeaders,
} from './http_internal';
import {
  exportContext,
  registerExport,
  type ModuleExport,
  type SchemaInner,
} from './schema';

export interface Request {
  readonly method: string;
  readonly url: string;
  readonly headers: Headers;
  readonly body: Uint8Array;
}

export type HttpHandler = (request: Request) => SyncResponse;
export type HttpHandlerExport = HttpHandler & ModuleExport;

type HttpMethodName = 'GET' | 'POST';

type Route = {
  method: HttpMethodName;
  path: string;
  handler: HttpHandlerExport;
};

export type HttpHandlers = HttpHandler[];

const responseBaseSize = bsatnBaseSize(
  { types: [] },
  HttpResponse.algebraicType
);

const routesSymbol = Symbol('SpacetimeDB.http.routes');
const httpHandlerSymbol = Symbol('SpacetimeDB.http.handler');

type RouterWithRoutes = Router & {
  [routesSymbol]: Route[];
};

const METHODS: Record<HttpMethodName, MethodOrAny> = {
  GET: { tag: 'Method', value: { tag: 'Get' } },
  POST: { tag: 'Method', value: { tag: 'Post' } },
};

function methodToString(method: HttpMethod): string {
  switch (method.tag) {
    case 'Get':
      return 'GET';
    case 'Head':
      return 'HEAD';
    case 'Post':
      return 'POST';
    case 'Put':
      return 'PUT';
    case 'Delete':
      return 'DELETE';
    case 'Connect':
      return 'CONNECT';
    case 'Options':
      return 'OPTIONS';
    case 'Trace':
      return 'TRACE';
    case 'Patch':
      return 'PATCH';
    case 'Extension':
      return method.value;
  }
}

export class Router {
  [routesSymbol]: Route[] = [];

  get(path: string, handler: HttpHandlerExport): this {
    // TODO(v8-http-handlers): Validate route path and duplicate registrations.
    this[routesSymbol].push({ method: 'GET', path, handler });
    return this;
  }

  post(path: string, handler: HttpHandlerExport): this {
    // TODO(v8-http-handlers): Validate route path and duplicate registrations.
    this[routesSymbol].push({ method: 'POST', path, handler });
    return this;
  }
}

function registerHttpHandler(
  ctx: SchemaInner,
  exportName: string,
  fn: HttpHandler
): void {
  ctx.defineFunction(exportName);
  ctx.moduleDef.httpHandlers.push({ sourceName: exportName });
  ctx.httpHandlers.push(fn);
}

function makeHttpHandlerExport(
  ctx: SchemaInner,
  fn: HttpHandler
): HttpHandlerExport {
  const handlerExport = fn as HttpHandlerExport & {
    [httpHandlerSymbol]?: true;
  };

  handlerExport[exportContext] = ctx;
  handlerExport[httpHandlerSymbol] = true;
  handlerExport[registerExport] = (ctx, exportName) => {
    // TODO(v8-http-handlers): Reject duplicate registration of the same function object.
    registerHttpHandler(ctx, exportName, fn);
    ctx.functionExports.set(handlerExport, exportName);
  };

  return handlerExport;
}

function makeHttpRouterExport(ctx: SchemaInner, router: Router): ModuleExport {
  return {
    [exportContext]: ctx,
    [registerExport](ctx, _exportName) {
      for (const route of (router as RouterWithRoutes)[routesSymbol]) {
        // TODO(v8-http-handlers): Verify that handlers referenced by routers come from the same schema.
        const handlerName = ctx.functionExports.get(route.handler);
        if (handlerName === undefined) {
          throw new TypeError(
            'HTTP router references a handler that was not exported as an HTTP handler'
          );
        }
        ctx.moduleDef.httpRoutes.push({
          handlerFunction: handlerName,
          method: METHODS[route.method],
          path: route.path,
        });
      }
    },
  };
}

export function makeHttpNamespace(ctx: SchemaInner) {
  return Object.freeze({
    Router,
    handler(fn: HttpHandler): HttpHandlerExport {
      return makeHttpHandlerExport(ctx, fn);
    },
    router(router: Router): ModuleExport {
      if (!(router instanceof Router)) {
        throw new TypeError('spacetime.http.router expects a Router instance');
      }
      return makeHttpRouterExport(ctx, router);
    },
  });
}

export function deserializeHttpHandlerRequest(
  requestBuf: Uint8Array,
  requestBody: Uint8Array
): Request {
  const request = HttpRequest.deserialize(new BinaryReader(requestBuf));
  return Object.freeze({
    method: methodToString(request.method),
    url: request.uri,
    headers: deserializeHeaders(request.headers),
    body: requestBody,
  });
}

export function serializeHttpHandlerResponse(
  response: SyncResponse
): [Uint8Array, Uint8Array] {
  const responseWire: HttpResponse = {
    code: response.status,
    headers: serializeHeaders(response.headers),
    version: { tag: 'Http11' },
  };

  const responseBuf = new BinaryWriter(responseBaseSize);
  HttpResponse.serialize(responseBuf, responseWire);
  return [responseBuf.getBuffer(), response.bytes()];
}
