import { AlgebraicType } from '../lib/algebraic_type';
import { FunctionVisibility, type Lifecycle } from '../lib/autogen/types';
import type { ParamsObj, Reducer } from '../lib/reducers';
import { type UntypedSchemaDef } from '../lib/schema';
import { RowBuilder, type RowObj } from '../lib/type_builders';
import { toPascalCase } from '../lib/util';
import {
  exportContext,
  registerExport,
  type ModuleExport,
  type SchemaInner,
} from './schema';

export interface ReducerExport<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
> extends Reducer<S, Params>,
    ModuleExport {}

export interface ReducerOpts {
  name: string;
}

export function makeReducerExport<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
>(
  ctx: SchemaInner,
  opts: ReducerOpts | undefined,
  params: RowObj | RowBuilder<RowObj>,
  fn: Reducer<any, any>,
  lifecycle?: Lifecycle
): ReducerExport<S, Params> {
  const reducerExport: ReducerExport<S, Params> = (...args) => fn(...args);
  reducerExport[exportContext] = ctx;
  reducerExport[registerExport] = (ctx, exportName) => {
    registerReducer(ctx, exportName, params, fn, opts, lifecycle);
    ctx.functionExports.set(
      reducerExport as ReducerExport<any, any>,
      exportName
    );
  };

  return reducerExport;
}

/**
 * internal: pushReducer() helper used by reducer() and lifecycle wrappers
 *
 * @param name - The name of the reducer.
 * @param params - The parameters for the reducer.
 * @param fn - The reducer function.
 * @param lifecycle - Optional lifecycle hooks for the reducer.
 */
export function registerReducer(
  ctx: SchemaInner,
  exportName: string,
  params: RowObj | RowBuilder<RowObj>,
  fn: Reducer<any, any>,
  opts?: ReducerOpts,
  lifecycle?: Lifecycle
): void {
  ctx.defineFunction(exportName);

  if (!(params instanceof RowBuilder)) {
    params = new RowBuilder(params);
  }

  if (params.typeName === undefined) {
    params.typeName = toPascalCase(exportName);
  }

  const ref = ctx.registerTypesRecursively(params);
  const paramsType = ctx.resolveType(ref).value;
  const isLifecycle = lifecycle != null;

  ctx.moduleDef.reducers.push({
    sourceName: exportName,
    params: paramsType,
    //ModuleDef validation code is responsible to mark private reducers
    visibility: FunctionVisibility.ClientCallable,
    //Hardcoded for now - reducers do not return values yet
    okReturnType: AlgebraicType.Product({ elements: [] }),
    errReturnType: AlgebraicType.String,
  });

  if (opts?.name != null) {
    ctx.moduleDef.explicitNames.entries.push({
      tag: 'Function',
      value: {
        sourceName: exportName,
        canonicalName: opts.name,
      },
    });
  }

  if (isLifecycle) {
    ctx.moduleDef.lifeCycleReducers.push({
      lifecycleSpec: lifecycle,
      functionName: exportName,
    });
  }

  // If the function isn't named (e.g. `function foobar() {}`), give it the same
  // name as the reducer so that it's clear what it is in in backtraces.
  if (!fn.name) {
    Object.defineProperty(fn, 'name', { value: exportName, writable: false });
  }

  ctx.reducers.push(fn);
}

export type Reducers = Reducer<any, any>[];
