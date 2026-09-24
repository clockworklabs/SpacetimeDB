import { beforeEach, describe, expect, it, vi } from 'vitest';

const host = vi.hoisted(() => ({
  flags: 0,
  payload: '',
  jwtReads: 0,
  flagReads: 0,
  conflicts: 0,
}));
vi.mock('spacetime:sys@2.0', () => ({
  moduleHooks: Symbol('moduleHooks'),
  identity: () => 1n,
  row_iter_bsatn_close: () => {},
  procedure_start_mut_tx: () => 0n,
  procedure_commit_mut_tx: () => {
    if (host.conflicts-- > 0) throw new Error('transaction conflict');
  },
  procedure_abort_mut_tx: () => {},
  get_jwt_payload: () => {
    host.jwtReads++;
    return new TextEncoder().encode(host.payload);
  },
}));
vi.mock('spacetime:sys@2.3', () => ({
  get_call_auth_flags: () => {
    host.flagReads++;
    return host.flags;
  },
}));

// Load procedures first so its existing runtime import cycle initializes in order.
import { callProcedure } from '../src/server/procedures';
import { ReducerCtxImpl } from '../src/server/runtime';
import { ConnectionId } from '../src/lib/connection_id';
import { Identity } from '../src/lib/identity';
import { Timestamp } from '../src/lib/timestamp';
import { schema, exportContext, registerExport } from '../src/server/schema';
import { t } from '../src/lib/type_builders';

beforeEach(() => {
  Object.assign(host, {
    flags: 0,
    payload: '',
    jwtReads: 0,
    flagReads: 0,
    conflicts: 0,
  });
});

describe('verified invocation authentication', () => {
  it.each([0, 1])(
    'preserves flag %s for calls without a connection or JWT',
    flags => {
      host.flags = flags;
      const ctx = new ReducerCtxImpl(
        new Identity(1n),
        Timestamp.UNIX_EPOCH,
        null,
        {}
      );
      expect(host.flagReads).toBe(0);
      expect(ctx.senderAuth.isInternal).toBe(Boolean(flags));
      expect(host.flagReads).toBe(1);
      expect(ctx.senderAuth.hasJWT).toBe(false);
      expect(ctx.senderAuth.jwt).toBeNull();
      expect(host.jwtReads).toBe(0);
    }
  );

  it('retains an internal connection and JWT independently, using the verified sender Identity', () => {
    host.flags = 1;
    host.payload = JSON.stringify({
      iss: 'unrelated-issuer',
      sub: 'unrelated-subject',
      identity: 'untrusted-claim',
    });
    const sender = new Identity(123n);
    const connection = new ConnectionId(7n);
    const ctx = new ReducerCtxImpl(
      sender,
      Timestamp.UNIX_EPOCH,
      connection,
      {}
    );
    expect(ctx.connectionId).toBe(connection);
    expect(ctx.senderAuth.isInternal).toBe(true);
    expect(host.jwtReads).toBe(0);
    expect(ctx.senderAuth.hasJWT).toBe(true);
    expect(ctx.senderAuth.jwt?.identity).toBe(sender);
    expect(ctx.senderAuth.jwt?.subject).toBe('unrelated-subject');
    expect(host.jwtReads).toBe(1);
  });

  it('refreshes captured flags and sender when a cached reducer context is reused', () => {
    host.flags = 1;
    const ctx = new ReducerCtxImpl(
      new Identity(1n),
      Timestamp.UNIX_EPOCH,
      null,
      {}
    );
    const firstAuth = ctx.senderAuth;
    host.flags = 0;
    ReducerCtxImpl.reset(
      ctx,
      new Identity(2n),
      Timestamp.UNIX_EPOCH,
      new ConnectionId(8n)
    );
    expect(firstAuth.isInternal).toBe(true);
    expect(ctx.senderAuth.isInternal).toBe(false);
    expect(ctx.senderAuth.hasJWT).toBe(false);
  });

  it.each([0, 1])(
    'reads invocation flag %s in every procedure transaction retry',
    flags => {
      host.flags = flags;
      host.conflicts = 1;
      const sender = new Identity(9n);
      const connection = new ConnectionId(8n);
      let attempts = 0;
      const module = schema({});
      const proc = module.procedure(t.unit(), ctx => {
        ctx.withTx(tx => {
          attempts++;
          expect(tx.senderAuth.isInternal).toBe(Boolean(flags));
          expect(tx.sender).toBe(sender);
          expect(tx.connectionId).toBe(connection);
        });
        return {};
      });
      const inner = proc[exportContext]!;
      proc[registerExport](inner, 'procedure_auth');
      callProcedure(
        inner.procedures,
        0,
        sender,
        connection,
        Timestamp.UNIX_EPOCH,
        new Uint8Array(),
        () => ({})
      );
      expect(attempts).toBe(2);
      expect(host.flagReads).toBe(2);
    }
  );
});

describe('hosted authentication capability', () => {
  it('advertises the updated bindings without changing function visibility', () => {
    const module = schema({});
    const run = module.reducer(() => {});
    const inner = run[exportContext]!;
    run[registerExport](inner, 'run');
    expect(inner.moduleDef.capabilities).toContain('hosted_auth_v1');
    expect(inner.moduleDef.reducers[0].visibility.tag).toBe('ClientCallable');
  });
});
