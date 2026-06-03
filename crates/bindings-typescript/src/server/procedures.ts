import {
  AlgebraicType,
  ProductType,
  type Deserializer,
  type Serializer,
} from '../lib/algebraic_type';
import { FunctionVisibility } from '../lib/autogen/types';
import BinaryReader from '../lib/binary_reader';
import BinaryWriter from '../lib/binary_writer';
import type { ConnectionId } from '../lib/connection_id';
import { Identity } from '../lib/identity';
import type { ParamsObj, ReducerCtx } from '../lib/reducers';
import { type UntypedSchemaDef } from '../lib/schema';
import { Timestamp } from '../lib/timestamp';
import {
  type Infer,
  type InferTypeOfRow,
  type TypeBuilder,
} from '../lib/type_builders';
import { bsatnBaseSize } from '../lib/util';
import { Uuid } from '../lib/uuid';
import { httpClient, type HttpClient } from './http_internal';
import type { DbView } from './db_view';
import { makeRandom, type Random } from './rng';
import { assignTxAliasViews, buildProcedureAliasCtxMap, callUserFunction, ReducerCtxImpl, runWithTx, sys } from './runtime';
import {
  exportContext,
  registerExport,
  type MountedDispatchInfo,
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
  opts: ProcedureOpts | undefined,
  params: Params,
  ret: Ret,
  fn: ProcedureFn<S, Params, Ret>
): ProcedureExport<S, Params, Ret> {
  const name = opts?.name;

  const procedureExport: ProcedureExport<S, Params, Ret> = (...args) =>
    fn(...args);
  procedureExport[exportContext] = ctx;
  procedureExport[registerExport] = (ctx, exportName) => {
    registerProcedure(ctx, name ?? exportName, params, ret, fn);
    ctx.functionExports.set(
      procedureExport as ProcedureExport<any, any, any>,
      name ?? exportName
    );
  };

  return procedureExport;
}

export type ProcedureFn<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
> = (ctx: ProcedureCtx<S>, args: InferTypeOfRow<Params>) => Infer<Ret>;

export interface ProcedureOpts {
  name: string;
}

export type ProcedureAliasViews<SchemaDef extends UntypedSchemaDef> =
  SchemaDef extends { namespaces: infer NS extends Record<string, UntypedSchemaDef> }
    ? { readonly [K in keyof NS]: ProcedureCtx<NS[K]> }
    : {};

export interface ProcedureCtx<S extends UntypedSchemaDef> {
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

type ITransactionCtx<S extends UntypedSchemaDef> = TransactionCtx<S>;

const TransactionCtxImpl = class TransactionCtx<S extends UntypedSchemaDef>
  extends ReducerCtxImpl<S>
  implements ITransactionCtx<S> {};

function registerProcedure<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
>(
  ctx: SchemaInner,
  exportName: string,
  params: Params,
  ret: Ret,
  fn: ProcedureFn<S, Params, Ret>,
  opts?: ProcedureOpts
) {
  ctx.defineFunction(exportName);
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
    fn,
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

export function callProcedure(
  procedures: Procedures,
  id: number,
  sender: Identity,
  connectionId: ConnectionId | null,
  timestamp: Timestamp,
  argsBuf: Uint8Array,
  dbView: () => DbView<any>,
  dispatches: MountedDispatchInfo[] = []
): Uint8Array {
  const { fn, deserializeArgs, serializeReturn, returnTypeBaseSize } =
    procedures[id];
  const args = deserializeArgs(new BinaryReader(argsBuf));

  const ctx: ProcedureCtx<UntypedSchemaDef> = new ProcedureCtxImpl(
    sender,
    timestamp,
    connectionId,
    dbView,
    dispatches
  );

  const ret = callUserFunction(fn, ctx, args);
  const retBuf = new BinaryWriter(returnTypeBaseSize);
  serializeReturn(retBuf, ret);
  return retBuf.getBuffer();
}

type IProcedureCtx<S extends UntypedSchemaDef> = ProcedureCtx<S>;
const ProcedureCtxImpl = class ProcedureCtx<S extends UntypedSchemaDef>
  implements IProcedureCtx<S>
{
  #identity: Identity | undefined;
  #uuidCounter: { value: 0 } | undefined;
  #random: Random | undefined;
  #dbView: () => DbView<any>;
  #dispatches: MountedDispatchInfo[];
  #asViews: object | undefined;

  constructor(
    readonly sender: Identity,
    readonly timestamp: Timestamp,
    readonly connectionId: ConnectionId | null,
    dbView: () => DbView<any>,
    dispatches: MountedDispatchInfo[] = []
  ) {
    this.#dbView = dbView;
    this.#dispatches = dispatches;
  }

  get databaseIdentity() {
    return (this.#identity ??= new Identity(sys.identity()));
  }

  get identity() {
    return this.databaseIdentity;
  }

  get random() {
    return (this.#random ??= makeRandom(this.timestamp));
  }

  get http() {
    return httpClient;
  }

  get as() {
    return (this.#asViews ??= buildProcedureAliasCtxMap(this, this.#dispatches, '')) as any;
  }

  withTx<T>(body: (ctx: TransactionCtx<S>) => T): T {
    const dispatches = this.#dispatches;
    return runWithTx(
      timestamp => {
        const tx = new TransactionCtxImpl(
          this.sender,
          timestamp,
          this.connectionId,
          this.#dbView()
        );
        assignTxAliasViews(tx, dispatches);
        return tx as unknown as TransactionCtx<S>;
      },
      body
    );
  }

  newUuidV4(): Uuid {
    const bytes = this.random.fill(new Uint8Array(16));
    return Uuid.fromRandomBytesV4(bytes);
  }

  newUuidV7(): Uuid {
    const bytes = this.random.fill(new Uint8Array(4));
    const counter = (this.#uuidCounter ??= { value: 0 });
    return Uuid.fromCounterV7(counter, this.timestamp, bytes);
  }
};
