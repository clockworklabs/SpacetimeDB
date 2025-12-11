import { AlgebraicType, ProductType } from '../lib/algebraic_type';
import type Typespace from '../lib/autogen/typespace_type';
import BinaryReader from '../lib/binary_reader';
import BinaryWriter from '../lib/binary_writer';
import type { ConnectionId } from '../lib/connection_id';
import { Identity } from '../lib/identity';
import type { ParamsObj, ReducerCtx } from '../lib/reducers';
import { ModuleContext, type UntypedSchemaDef } from '../lib/schema';
import { Timestamp } from '../lib/timestamp';
import {
  type Infer,
  type InferTypeOfRow,
  type TypeBuilder,
} from '../lib/type_builders';
import { bsatnBaseSize } from '../lib/util';
import type { HttpClient } from '../server/http_internal';
import { httpClient } from './http_internal';
import { callUserFunction, makeReducerCtx, sys } from './runtime';

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
  withTx<T>(body: (ctx: TransactionCtx<S>) => T): T;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface TransactionCtx<S extends UntypedSchemaDef>
  extends ReducerCtx<S> {}

export function procedure<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
>(
  ctx: ModuleContext,
  name: string,
  params: Params,
  ret: Ret,
  fn: ProcedureFn<S, Params, Ret>
) {
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

  PROCEDURES.push({
    fn,
    paramsType,
    returnType,
    returnTypeBaseSize: bsatnBaseSize(ctx.typespace, returnType),
  });
}

export const PROCEDURES: Array<{
  fn: ProcedureFn<any, any, any>;
  paramsType: ProductType;
  returnType: AlgebraicType;
  returnTypeBaseSize: number;
}> = [];

export function callProcedure(
  typespace: Infer<typeof Typespace>,
  id: number,
  sender: Identity,
  connectionId: ConnectionId | null,
  timestamp: Timestamp,
  argsBuf: Uint8Array
): Uint8Array {
  const { fn, paramsType, returnType, returnTypeBaseSize } = PROCEDURES[id];
  const args = ProductType.deserializeValue(
    new BinaryReader(argsBuf),
    paramsType,
    typespace
  );

  const ctx: ProcedureCtx<UntypedSchemaDef> = {
    sender,
    timestamp,
    connectionId,
    http: httpClient,
    get identity() {
      return new Identity(sys.identity().__identity__);
    },
    withTx(body) {
      const run = () => {
        const timestamp = sys.procedure_start_mut_tx();

        try {
          const ctx: TransactionCtx<UntypedSchemaDef> = freeze(
            makeReducerCtx(sender, new Timestamp(timestamp), connectionId)
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
  };
  freeze(ctx);

  const ret = callUserFunction(fn, ctx, args);
  const retBuf = new BinaryWriter(returnTypeBaseSize);
  AlgebraicType.serializeValue(retBuf, returnType, ret, typespace);
  return retBuf.getBuffer();
}
