import type { EnvironmentFor } from './environment';
import {
  AlgebraicType,
  ProductType,
  type Deserializer,
  type Serializer,
} from '../lib/algebraic_type';
import { FunctionVisibility } from '../lib/autogen/types';
import type { ConnectionId } from '../lib/connection_id';
import { Identity } from '../lib/identity';
import type { ParamsObj, ReducerCtx } from '../lib/reducers';
import { type UntypedSchemaDef } from '../lib/schema';
import type { ScheduleTableForParams } from '../lib/table_schema';
import { Timestamp } from '../lib/timestamp';
import {
  type Infer,
  type InferTypeOfRow,
  type t,
  type TypeBuilder,
} from '../lib/type_builders';
import { bsatnBaseSize } from '../lib/util';
import { Uuid } from '../lib/uuid';
import type { HttpClient } from './http_internal';
import type { Random } from './rng';
import {
  exportContext,
  registerExport,
  type ModuleExport,
  type SchemaInner,
} from './schema';

export type ProcedureExport<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
> = ProcedureFn<S, Params, Ret> & ModuleExport;

export function makeProcedureExport<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
>(
  ctx: SchemaInner,
  opts: ProcedureOptsWithOptionalName<Params, Ret> | undefined,
  params: Params,
  ret: Ret,
  fn: ProcedureFn<S, Params, Ret> | undefined
): ProcedureExport<S, Params, Ret> {
  const name = opts?.name;

  const procedureExport: ProcedureExport<S, Params, Ret> = (...args) =>
    fn!(...args);
  procedureExport[exportContext] = ctx;
  procedureExport[registerExport] = (ctx, exportName) => {
    registerProcedure(ctx, name ?? exportName, params, ret, fn);
    ctx.functionExports.set(
      procedureExport as ProcedureExport<any, any, any>,
      name ?? exportName
    );
    if (opts?.onSchedule !== undefined) {
      ctx.pendingSchedules.push({
        table: opts.onSchedule,
        functionName: name ?? exportName,
      });
    }
  };

  return procedureExport;
}

export type ProcedureFn<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
> = (ctx: ProcedureCtx<S>, args: InferTypeOfRow<Params>) => Infer<Ret>;

export interface ProcedureOpts<
  Params extends ParamsObj = ParamsObj,
  Ret extends TypeBuilder<any, any> = TypeBuilder<any, any>,
  Name extends string = string,
> {
  name: Name;
  onSchedule?: Ret extends ReturnType<typeof t.unit>
    ? ScheduleTableForParams<Params>
    : never;
}

export type ProcedureOptsWithOptionalName<
  Params extends ParamsObj = ParamsObj,
  Ret extends TypeBuilder<any, any> = TypeBuilder<any, any>,
  Name extends string = string,
> = Omit<ProcedureOpts<Params, Ret, Name>, 'name'> & { name?: Name };

/** What a client reads from a procedure declaration. See `moduleDefFromExports`. */
export type ProcedureSignature<
  Name extends string,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
> = {
  readonly kind: 'procedure';
  /** The canonical name, if the declaration gives one. */
  readonly name: Name | undefined;
  readonly params: Params;
  readonly returnType: Ret;
};

export type ProcedureAliasViews<SchemaDef extends UntypedSchemaDef> =
  SchemaDef extends {
    namespaces: infer NS extends Record<string, UntypedSchemaDef>;
  }
    ? { readonly [K in keyof NS]: ProcedureCtx<NS[K]> }
    : {};

export interface ProcedureCtx<S extends UntypedSchemaDef> {
  readonly env: EnvironmentFor<S>;
  readonly sender: Identity;
  readonly databaseIdentity: Identity;
  /** @deprecated Use `databaseIdentity` instead. */
  readonly identity: Identity;
  readonly timestamp: Timestamp;
  readonly connectionId: ConnectionId | null;
  readonly http: HttpClient;
  readonly random: Random;
  readonly as: ProcedureAliasViews<S>;
  withTx<T>(body: (ctx: TransactionCtx<S>) => T): T;
  newUuidV4(): Uuid;
  newUuidV7(): Uuid;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface TransactionCtx<S extends UntypedSchemaDef>
  extends ReducerCtx<S> {}

function registerProcedure<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
>(
  ctx: SchemaInner,
  exportName: string,
  params: Params,
  ret: Ret,
  fn: ProcedureFn<S, Params, Ret> | undefined,
  opts?: ProcedureOptsWithOptionalName<any, any>
) {
  ctx.defineFunction(exportName);
  ctx.recordMissingBody('procedure', exportName, fn);
  const paramsType: ProductType = {
    elements: Object.entries(params).map(([n, c]) => ({
      name: n,
      algebraicType: ctx.registerTypesRecursively(
        'typeBuilder' in c ? c.typeBuilder : c
      ).algebraicType,
    })),
  };
  const returnType = ctx.registerTypesRecursively(ret).algebraicType;

  ctx.moduleDef.procedures.push({
    sourceName: exportName,
    params: paramsType,
    returnType,
    visibility: FunctionVisibility.ClientCallable,
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
  const { typespace } = ctx;

  ctx.procedures.push({
    // Only a client registers a procedure without a body, and it never runs it.
    fn: fn!,
    deserializeArgs: ProductType.makeDeserializer(paramsType, typespace),
    serializeReturn: AlgebraicType.makeSerializer(returnType, typespace),
    returnTypeBaseSize: bsatnBaseSize(typespace, returnType),
  });
}

export type Procedures = Array<{
  fn: ProcedureFn<any, any, any>;
  deserializeArgs: Deserializer<any>;
  serializeReturn: Serializer<any>;
  returnTypeBaseSize: number;
}>;
