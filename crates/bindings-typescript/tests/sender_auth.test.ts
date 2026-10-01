import { describe, expect, it } from 'vitest';
import * as sys from 'spacetime:sys@2.0';
import { schema } from '../src/server/schema';
import { ReducerCtxImpl } from '../src/server/runtime';
import { table } from '../src/lib/table';
import { t } from '../src/lib/type_builders';
import { Timestamp } from '../src/lib/timestamp';

describe('ctx.senderAuth', () => {
  function senderAuthFor(connectionId: bigint) {
    const items = table({ name: 'items' }, { id: t.u32().primaryKey() });
    const module = schema({ items });
    let isInternal: boolean | undefined;
    const check = module.reducer(ctx => {
      isInternal = ctx.senderAuth.isInternal;
    });
    const hooks = (module as any)[(sys as any).moduleHooks]({ check });
    hooks.__call_reducer__(
      0,
      0n,
      connectionId,
      0n,
      new DataView(new ArrayBuffer(0))
    );
    return isInternal;
  }

  it('is internal for a reducer the database spawns without a connection', () => {
    expect(senderAuthFor(0n)).toBe(true);
  });

  it('is external for a reducer called over a connection', () => {
    expect(senderAuthFor(7n)).toBe(false);
  });

  it('is external for a transaction inside an HTTP handler', () => {
    const tx = ReducerCtxImpl.forHttpHandler(new Timestamp(0n), {} as never);
    expect(tx.connectionId).toBeNull();
    expect(tx.senderAuth.isInternal).toBe(false);
    expect(tx.senderAuth.hasJWT).toBe(false);
  });
});
