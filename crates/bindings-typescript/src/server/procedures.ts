import { AlgebraicType, ProductType } from '../lib/algebraic_type';
import BinaryReader from '../lib/binary_reader';
import BinaryWriter from '../lib/binary_writer';
import type { ConnectionId } from '../lib/connection_id';
import type { Identity } from '../lib/identity';
import type { Timestamp } from '../lib/timestamp';
import type { ParamsObj } from './reducers';
import { MODULE_DEF, type UntypedSchemaDef } from './schema';
import type { Infer, InferTypeOfRow, TypeBuilder } from './type_builders';
import { bsatnBaseSize } from './util';

export type ProcedureFn<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
> = (ctx: ProcedureCtx<S>, args: InferTypeOfRow<Params>) => Infer<Ret>;

export type ProcedureCtx<S extends UntypedSchemaDef> = Readonly<{
  sender: Identity;
  timestamp: Timestamp;
  connectionId: ConnectionId | null;
}>;

export function procedure<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
>(name: string, params: Params, ret: Ret, fn: ProcedureFn<S, Params, Ret>) {
  const paramsType: ProductType = {
    elements: Object.entries(params).map(([n, c]) => ({
      name: n,
      algebraicType: c.algebraicType,
    })),
  };
  const returnType = ret.algebraicType;

  MODULE_DEF.miscExports.push({
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
    returnTypeBaseSize: bsatnBaseSize(MODULE_DEF.typespace, returnType),
  });
}

const PROCEDURES: Array<{
  fn: ProcedureFn<any, any, any>;
  paramsType: ProductType;
  returnType: AlgebraicType;
  returnTypeBaseSize: number;
}> = [];

export function callProcedure(
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
    MODULE_DEF.typespace
  );
  const ret = fn(Object.freeze({ sender, connectionId, timestamp }), args);
  const retBuf = new BinaryWriter(returnTypeBaseSize);
  AlgebraicType.serializeValue(retBuf, returnType, ret, MODULE_DEF.typespace);
  return retBuf.getBuffer();
}
