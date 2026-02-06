import {
  AlgebraicType,
  ProductType,
  type Deserializer,
  type Serializer,
} from '../lib/algebraic_type';
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
import type { HttpClient } from '../server/http_internal';
import { httpClient } from './http_internal';
import { callUserFunction, ReducerCtxImpl, sys } from './runtime';
import type { SchemaInner } from './schema';

const { freeze } = Object;

export type ProcedureFn<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
> = (ctx: ProcedureCtx<S>, args: InferTypeOfRow<Params>) => Infer<Ret>;

export interface ProcedureCtx<S extends UntypedSchemaDef> {
  readonly sender: Identity;
  readonly identity: Identity;
  readonly timestamp: Timestamp;
  readonly connectionId: ConnectionId | null;
  readonly http: HttpClient;
  readonly counter_uuid: { value: number };
  withTx<T>(body: (ctx: TransactionCtx<S>) => T): T;
  newUuidV4(): Uuid;
  newUuidV7(): Uuid;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface TransactionCtx<S extends UntypedSchemaDef>
  extends ReducerCtx<S> {}

export function procedure<
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

  ctx.moduleDef.miscExports.push({
    tag: 'Procedure',
    value: {
      name,
      params: paramsType,
      returnType,
    },
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
  argsBuf: Uint8Array
): Uint8Array {
  const { fn, deserializeArgs, serializeReturn, returnTypeBaseSize } =
    moduleCtx.procedures[id];
  const args = deserializeArgs(new BinaryReader(argsBuf));

  const ctx: ProcedureCtx<UntypedSchemaDef> = {
    sender,
    timestamp,
    connectionId,
    http: httpClient,
    // **Note:** must be 0..=u32::MAX
    counter_uuid: { value: Number(0) },
    get identity() {
      return new Identity(sys.identity());
    },
    withTx(body) {
      const run = () => {
        const timestamp = sys.procedure_start_mut_tx();

        try {
          const ctx: TransactionCtx<UntypedSchemaDef> = new ReducerCtxImpl(
            sender,
            new Timestamp(timestamp),
            connectionId
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
    },
    /**
     * Create a new random {@link Uuid} `v4` using the {@link crypto} RNG.
     *
     * WARN: Until we use a spacetime RNG this make calls non-deterministic.
     */
    newUuidV4(): Uuid {
      // TODO: Use a spacetime RNG when available
      const bytes = crypto.getRandomValues(new Uint8Array(16));
      return Uuid.fromRandomBytesV4(bytes);
    },

    /**
     * Create a new sortable {@link Uuid} `v7` using the {@link crypto} RNG, counter,
     * and the timestamp.
     *
     * WARN: Until we use a spacetime RNG this make calls non-deterministic.
     */
    newUuidV7(): Uuid {
      // TODO: Use a spacetime RNG when available
      const bytes = crypto.getRandomValues(new Uint8Array(10));
      return Uuid.fromCounterV7(this.counter_uuid, this.timestamp, bytes);
    },
  };
  freeze(ctx);

  const ret = callUserFunction(fn, ctx, args);
  const retBuf = new BinaryWriter(returnTypeBaseSize);
  serializeReturn(retBuf, ret);
  return retBuf.getBuffer();
}
