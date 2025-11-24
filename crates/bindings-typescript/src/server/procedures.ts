import { AlgebraicType, ProductType } from '../lib/algebraic_type';
import BinaryReader from '../lib/binary_reader';
import BinaryWriter from '../lib/binary_writer';
import type { ConnectionId } from '../lib/connection_id';
import { Identity } from '../lib/identity';
import { PROCEDURES, type ProcedureCtx } from '../lib/procedures';
import { MODULE_DEF, type UntypedSchemaDef } from '../lib/schema';
import type { Timestamp } from '../lib/timestamp';
import { sys } from './runtime';

const { freeze } = Object;

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

  const ctx: ProcedureCtx<UntypedSchemaDef> = freeze({
    sender,
    timestamp,
    connectionId,
    get identity() {
      return new Identity(sys.identity().__identity__);
    },
  });

  const ret = fn(ctx, args);
  const retBuf = new BinaryWriter(returnTypeBaseSize);
  AlgebraicType.serializeValue(retBuf, returnType, ret, MODULE_DEF.typespace);
  return retBuf.getBuffer();
}
