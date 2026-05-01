import type { Identity } from '../lib/identity';
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
import type { Request, Response } from './http_api';
import type { Router } from './http_router';

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
) => Response;

export const httpHandlerFn = Symbol('SpacetimeDB.httpHandlerFn');

export interface HttpHandlerExport<
  S extends UntypedSchemaDef = UntypedSchemaDef,
> extends ModuleExport {
  [httpHandlerFn]: HandlerFn<S>;
}

const exportedHttpHandlerObjects = new WeakSet<object>();

export interface HttpHandlerOpts {
  name: string;
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
