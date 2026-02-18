import { AlgebraicType } from '../lib/algebraic_type';
import { FunctionVisibility, type Lifecycle } from '../lib/autogen/types';
import type { ParamsObj, Reducer } from '../lib/reducers';
import { type UntypedSchemaDef } from '../lib/schema';
import { RowBuilder, type Infer, type RowObj } from '../lib/type_builders';
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
  lifecycle?: Infer<typeof Lifecycle>
): ReducerExport<S, Params> {
  const name = opts?.name;

  const reducerExport: ReducerExport<S, Params> = (...args) => fn(...args);
  reducerExport[exportContext] = ctx;
  reducerExport[registerExport] = (ctx, exportName) => {
    registerReducer(ctx, name ?? exportName, params, fn, lifecycle);
    ctx.functionExports.set(
      reducerExport as ReducerExport<any, any>,
      name ?? exportName
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
  name: string,
  params: RowObj | RowBuilder<RowObj>,
  fn: Reducer<any, any>,
  lifecycle?: Infer<typeof Lifecycle>
): void {
  ctx.defineFunction(name);

  if (!(params instanceof RowBuilder)) {
    params = new RowBuilder(params);
  }

  if (params.typeName === undefined) {
    params.typeName = toPascalCase(name);
  }

  const ref = ctx.registerTypesRecursively(params);
  const paramsType = ctx.resolveType(ref).value;
  const isLifecycle = lifecycle != null;

  ctx.moduleDef.reducers.push({
    sourceName: name,
    params: paramsType,
    //ModuleDef validation code is responsible to mark private reducers
    visibility: FunctionVisibility.ClientCallable,
    //Hardcoded for now - reducers do not return values yet
    okReturnType: AlgebraicType.Product({ elements: [] }),
    errReturnType: AlgebraicType.String,
  });

  if (isLifecycle) {
    ctx.moduleDef.lifeCycleReducers.push({
      lifecycleSpec: lifecycle,
      functionName: name,
    });
  }

  // If the function isn't named (e.g. `function foobar() {}`), give it the same
  // name as the reducer so that it's clear what it is in in backtraces.
  if (!fn.name) {
    Object.defineProperty(fn, 'name', { value: name, writable: false });
  }

  ctx.reducers.push(fn);
}

export type Reducers = Reducer<any, any>[];
