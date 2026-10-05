import { AlgebraicType } from '../lib/algebraic_type';
import { FunctionVisibility, type Lifecycle } from '../lib/autogen/types';
import type { ParamsObj, Reducer } from '../lib/reducers';
import { type UntypedSchemaDecl } from '../lib/schema';
import type { ScheduleTableForParams } from '../lib/table_body';
import { RowBuilder, type RowObj } from '../lib/type_builders';
import { toPascalCase } from '../lib/util';
import {
  exportContext,
  registerExport,
  type ModuleExport,
  type SchemaInner,
} from './schema';

export interface ReducerExport<
  S extends UntypedSchemaDecl,
  Params extends ParamsObj,
> extends Reducer<S, Params>,
    ModuleExport {}

export interface ReducerOpts<
  Params extends ParamsObj = ParamsObj,
  Name extends string = string,
> {
  name: Name;
  onSchedule?: ScheduleTableForParams<Params>;
}

export type ReducerOptsWithOptionalName<
  Params extends ParamsObj = ParamsObj,
  Name extends string = string,
> = Omit<ReducerOpts<Params, Name>, 'name'> & { name?: Name };

/** What a client reads from a reducer declaration. See `remoteModuleDeclFromExports`. */
export type ReducerSignature<Name extends string, Params extends ParamsObj> = {
  readonly kind: 'reducer';
  /** The canonical name, if the declaration gives one. */
  readonly name: Name | undefined;
  readonly params: Params;
};

export function makeReducerExport<
  S extends UntypedSchemaDecl,
  Params extends ParamsObj,
>(
  ctx: SchemaInner,
  opts: ReducerOptsWithOptionalName<Params> | undefined,
  params: RowObj | RowBuilder<RowObj>,
  fn: Reducer<any, any> | undefined,
  lifecycle?: Lifecycle
): ReducerExport<S, Params> {
  const reducerExport: ReducerExport<S, Params> = (...args) => fn!(...args);
  reducerExport[exportContext] = ctx;
  reducerExport[registerExport] = (ctx, exportName) => {
    registerReducer(ctx, exportName, params, fn, opts, lifecycle);
    ctx.functionExports.set(
      reducerExport as ReducerExport<any, any>,
      exportName
    );
    if (opts?.onSchedule !== undefined) {
      ctx.pendingSchedules.push({
        table: opts.onSchedule,
        functionName: exportName,
      });
    }
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
  fn: Reducer<any, any> | undefined,
  opts?: ReducerOptsWithOptionalName<any>,
  lifecycle?: Lifecycle
): void {
  ctx.defineFunction(exportName);
  ctx.requireBody('reducer', exportName, fn);

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
  if (fn && !fn.name) {
    Object.defineProperty(fn, 'name', { value: exportName, writable: false });
  }

  // Only a client registers a reducer without a body, and it never runs it.
  ctx.reducers.push(fn!);
}

export type Reducers = Reducer<any, any>[];
