import BinaryReader from '../lib/binary_reader';
import BinaryWriter from '../lib/binary_writer';
import type { ConnectionId } from '../lib/connection_id';
import { Identity } from '../lib/identity';
import {
  PROCEDURES,
  type ProcedureCtx,
  type TransactionCtx,
} from '../lib/procedures';
import { type UntypedSchemaDef } from '../lib/schema';
import { Timestamp } from '../lib/timestamp';
import { httpClient } from './http_internal';
import { callUserFunction, makeReducerCtx, sys } from './runtime';

const { freeze } = Object;

export function callProcedure(
  id: number,
  sender: Identity,
  connectionId: ConnectionId | null,
  timestamp: Timestamp,
  argsBuf: Uint8Array
): Uint8Array {
  const { fn, deserializeArgs, serializeReturn, returnTypeBaseSize } =
    PROCEDURES[id];
  const args = deserializeArgs(new BinaryReader(argsBuf));

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
  serializeReturn(retBuf, ret);
  return retBuf.getBuffer();
}
