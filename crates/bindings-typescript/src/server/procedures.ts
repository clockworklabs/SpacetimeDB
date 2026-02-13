import {
  AlgebraicType,
  ProductType,
  type Deserializer,
  type Serializer,
} from '../lib/algebraic_type';
import FunctionVisibility from '../lib/autogen/function_visibility_type';
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
import { callUserFunction, ReducerCtxImpl, sys } from './runtime';
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

export interface ProcedureCtx<S extends UntypedSchemaDef> {
  readonly sender: Identity;
  readonly identity: Identity;
  readonly timestamp: Timestamp;
  readonly connectionId: ConnectionId | null;
  readonly http: HttpClient;
  readonly random: Random;
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
  name: string,
  params: Params,
  ret: Ret,
  fn: ProcedureFn<S, Params, Ret>
) {
  ctx.defineFunction(name);
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
    sourceName: name,
    params: paramsType,
    returnType,
    visibility: FunctionVisibility.ClientCallable,
  });

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
  moduleCtx: SchemaInner,
  id: number,
  sender: Identity,
  connectionId: ConnectionId | null,
  timestamp: Timestamp,
  argsBuf: Uint8Array,
  dbView: () => DbView<any>
): Uint8Array {
  const { fn, deserializeArgs, serializeReturn, returnTypeBaseSize } =
    moduleCtx.procedures[id];
  const args = deserializeArgs(new BinaryReader(argsBuf));

  const ctx: ProcedureCtx<UntypedSchemaDef> = new ProcedureCtxImpl(
    sender,
    timestamp,
    connectionId,
    dbView
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

  constructor(
    readonly sender: Identity,
    readonly timestamp: Timestamp,
    readonly connectionId: ConnectionId | null,
    dbView: () => DbView<any>
  ) {
    this.#dbView = dbView;
  }

  get identity() {
    return (this.#identity ??= new Identity(sys.identity()));
  }

  get random() {
    return (this.#random ??= makeRandom(this.timestamp));
  }

  get http() {
    return httpClient;
  }

  withTx<T>(body: (ctx: TransactionCtx<S>) => T): T {
    const run = () => {
      const timestamp = sys.procedure_start_mut_tx();

      try {
        const ctx: TransactionCtx<S> = new TransactionCtxImpl(
          this.sender,
          new Timestamp(timestamp),
          this.connectionId,
          this.#dbView()
        );
        return body(ctx);
      } catch (e) {
        sys.procedure_abort_mut_tx();
        throw e;
      }
    };

    let res = run();
    try {
      sys.procedure_commit_mut_tx();
      return res;
    } catch {
      // ignore the commit error
    }
    console.warn('committing anonymous transaction failed');
    res = run();
    try {
      sys.procedure_commit_mut_tx();
      return res;
    } catch (e) {
      throw new Error('transaction retry failed again', { cause: e });
    }
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
