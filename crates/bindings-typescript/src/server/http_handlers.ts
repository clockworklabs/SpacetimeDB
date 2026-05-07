import type { Identity } from '../lib/identity';
import type {
  HttpMethod,
  HttpVersion,
  MethodOrAny,
} from '../lib/autogen/types';
import type { UntypedSchemaDef } from '../lib/schema';
import type { Timestamp } from '../lib/timestamp';
import type { Uuid } from '../lib/uuid';
import type { TransactionCtx } from './procedures';
import type { HttpClient } from './http_internal';
import type { Random } from './rng';
import {
  exportContext,
  registerExport,
  type ModuleExport,
  type SchemaInner,
} from './schema';
import {
  Headers,
  makeResponse,
  SyncResponse,
  textDecoder,
  textEncoder,
  type BodyInit,
  type HeadersInit,
  type ResponseInit,
} from './http_shared';

export { Headers };
export { SyncResponse };
export type { BodyInit, HeadersInit, ResponseInit };
export { makeResponse };
export const httpHandlerFn = Symbol('SpacetimeDB.httpHandlerFn');

export interface RequestInit {
  body?: BodyInit | null;
  headers?: HeadersInit;
  method?: string;
  version?: HttpVersion;
}

type RequestInner = {
  headers: Headers;
  method: string;
  uri: string;
  version: HttpVersion;
};

type RouteSpec = {
  handler: HttpHandlerExport<any>;
  method: MethodOrAny;
  path: string;
};

const ACCEPTABLE_ROUTE_PATH_CHARS_HUMAN_DESCRIPTION =
  'ASCII lowercase letters, digits and `-_~/`';

export const makeRequest = Symbol('makeRequest');

function coerceRequestBody(body?: BodyInit | null): string | Uint8Array | null {
  if (body == null) {
    return null;
  }
  if (typeof body === 'string') {
    return body;
  }
  return new Uint8Array(body as any);
}

function requestBodyToBytes(body: string | Uint8Array | null): Uint8Array {
  if (body == null) {
    return new Uint8Array();
  }
  if (typeof body === 'string') {
    return textEncoder.encode(body);
  }
  return body;
}

function requestBodyToText(body: string | Uint8Array | null): string {
  if (body == null) {
    return '';
  }
  if (typeof body === 'string') {
    return body;
  }
  return textDecoder.decode(body);
}

function characterIsAcceptableForRoutePath(c: string) {
  return (
    (c >= 'a' && c <= 'z') ||
    (c >= '0' && c <= '9') ||
    c === '-' ||
    c === '_' ||
    c === '~' ||
    c === '/'
  );
}

function assertValidPath(path: string) {
  if (path !== '' && !path.startsWith('/')) {
    throw new TypeError(`Route paths must start with \`/\`: ${path}`);
  }
  if (![...path].every(characterIsAcceptableForRoutePath)) {
    throw new TypeError(
      `Route paths may contain only ${ACCEPTABLE_ROUTE_PATH_CHARS_HUMAN_DESCRIPTION}: ${path}`
    );
  }
}

function routesOverlap(a: RouteSpec, b: RouteSpec) {
  const methodsMatch = (left: HttpMethod, right: HttpMethod) => {
    if (left.tag !== right.tag) {
      return false;
    }
    if (left.tag === 'Extension' && right.tag === 'Extension') {
      return left.value === right.value;
    }
    return true;
  };

  return (
    a.path === b.path &&
    (a.method.tag === 'Any' ||
      b.method.tag === 'Any' ||
      (a.method.tag === 'Method' &&
        b.method.tag === 'Method' &&
        methodsMatch(a.method.value, b.method.value)))
  );
}

function joinPaths(prefix: string, suffix: string) {
  if (prefix === '/') {
    return suffix;
  }
  if (suffix === '/') {
    return prefix;
  }
  const joinedPrefix = prefix.replace(/\/+$/, '');
  const joinedSuffix = suffix.replace(/^\/+/, '');
  return `${joinedPrefix}/${joinedSuffix}`;
}

export class Request {
  #body: string | Uint8Array | null;
  #inner: RequestInner;

  constructor(url: URL | string, init: RequestInit = {}) {
    this.#body = coerceRequestBody(init.body);
    this.#inner = {
      headers: new Headers(init.headers as any),
      method: init.method ?? 'GET',
      uri: '' + url,
      version: init.version ?? { tag: 'Http11' },
    };
  }

  static [makeRequest](body: BodyInit | null, inner: RequestInner) {
    const me = new Request(inner.uri);
    me.#body = coerceRequestBody(body);
    me.#inner = inner;
    return me;
  }

  get headers(): Headers {
    return this.#inner.headers;
  }

  get method(): string {
    return this.#inner.method;
  }

  get uri(): string {
    return this.#inner.uri;
  }

  get url(): string {
    return this.#inner.uri;
  }

  get version(): HttpVersion {
    return this.#inner.version;
  }

  arrayBuffer(): ArrayBuffer {
    return this.bytes().buffer as ArrayBuffer;
  }

  bytes(): Uint8Array {
    return requestBodyToBytes(this.#body);
  }

  json(): any {
    return JSON.parse(this.text());
  }

  text(): string {
    return requestBodyToText(this.#body);
  }
}

export interface HandlerContext<S extends UntypedSchemaDef = UntypedSchemaDef> {
  readonly timestamp: Timestamp;
  readonly http: HttpClient;
  readonly identity: Identity;
  readonly random: Random;
  withTx<T>(body: (ctx: TransactionCtx<S>) => T): T;
  newUuidV4(): Uuid;
  newUuidV7(): Uuid;
}

export type HandlerFn<S extends UntypedSchemaDef = UntypedSchemaDef> = (
  ctx: HandlerContext<S>,
  req: Request
) => SyncResponse;

export interface HttpHandlerExport<
  S extends UntypedSchemaDef = UntypedSchemaDef,
> extends ModuleExport {
  [httpHandlerFn]: HandlerFn<S>;
}

const exportedHttpHandlerObjects = new WeakSet<object>();

export interface HttpHandlerOpts {
  name: string;
}

export class Router {
  #routes: RouteSpec[];

  constructor(routes: RouteSpec[] = []) {
    this.#routes = routes;
  }

  get(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute(
      { tag: 'Method', value: { tag: 'Get' } },
      path,
      handler
    );
  }

  head(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute(
      { tag: 'Method', value: { tag: 'Head' } },
      path,
      handler
    );
  }

  options(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute(
      { tag: 'Method', value: { tag: 'Options' } },
      path,
      handler
    );
  }

  put(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute(
      { tag: 'Method', value: { tag: 'Put' } },
      path,
      handler
    );
  }

  delete(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute(
      { tag: 'Method', value: { tag: 'Delete' } },
      path,
      handler
    );
  }

  post(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute(
      { tag: 'Method', value: { tag: 'Post' } },
      path,
      handler
    );
  }

  patch(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute(
      { tag: 'Method', value: { tag: 'Patch' } },
      path,
      handler
    );
  }

  any(path: string, handler: HttpHandlerExport<any>) {
    return this.addRoute({ tag: 'Any' }, path, handler);
  }

  nest(path: string, subRouter: Router) {
    assertValidPath(path);
    if (this.#routes.some(route => route.path.startsWith(path))) {
      throw new TypeError(
        `Cannot nest router at \`${path}\`; existing routes overlap with nested path`
      );
    }

    let merged = new Router(this.#routes);
    for (const route of subRouter.#routes) {
      merged = merged.addRoute(
        route.method,
        joinPaths(path, route.path),
        route.handler
      );
    }
    return merged;
  }

  merge(otherRouter: Router) {
    let merged = new Router(this.#routes);
    for (const route of otherRouter.#routes) {
      merged = merged.addRoute(route.method, route.path, route.handler);
    }
    return merged;
  }

  intoRoutes() {
    return this.#routes.slice();
  }

  private addRoute(
    method: MethodOrAny,
    path: string,
    handler: HttpHandlerExport<any>
  ) {
    assertValidPath(path);
    const candidate = { method, path, handler };
    if (this.#routes.some(route => routesOverlap(route, candidate))) {
      throw new TypeError(`Route conflict for \`${path}\``);
    }
    return new Router([...this.#routes, candidate]);
  }
}

export function makeHttpHandlerExport<S extends UntypedSchemaDef>(
  ctx: SchemaInner,
  opts: HttpHandlerOpts | undefined,
  fn: HandlerFn<S>
): HttpHandlerExport<S> {
  const handlerExport = {
    [httpHandlerFn]: fn,
    [exportContext]: ctx,
    [registerExport](ctx: SchemaInner, exportName: string) {
      if (exportedHttpHandlerObjects.has(handlerExport)) {
        throw new TypeError(
          `HTTP handler '${exportName}' was exported more than once`
        );
      }
      exportedHttpHandlerObjects.add(handlerExport);
      registerHttpHandler(ctx, exportName, fn, opts);
      ctx.httpHandlerExports.set(
        handlerExport as HttpHandlerExport<UntypedSchemaDef>,
        exportName
      );
    },
  };
  return handlerExport as HttpHandlerExport<S>;
}

export function makeHttpRouterExport(
  ctx: SchemaInner,
  router: Router
): ModuleExport {
  return {
    [exportContext]: ctx,
    [registerExport](ctx: SchemaInner) {
      ctx.pendingHttpRoutes.push(...router.intoRoutes());
    },
  };
}

function registerHttpHandler<S extends UntypedSchemaDef>(
  ctx: SchemaInner,
  exportName: string,
  fn: HandlerFn<S>,
  opts?: HttpHandlerOpts
) {
  ctx.defineHttpHandler(exportName);
  ctx.moduleDef.httpHandlers.push({ sourceName: exportName });

  if (opts?.name != null) {
    ctx.moduleDef.explicitNames.entries.push({
      tag: 'Function',
      value: {
        sourceName: exportName,
        canonicalName: opts.name,
      },
    });
  }

  if (!fn.name) {
    Object.defineProperty(fn, 'name', { value: exportName, writable: false });
  }

  ctx.httpHandlers.push(fn as HandlerFn<UntypedSchemaDef>);
}
