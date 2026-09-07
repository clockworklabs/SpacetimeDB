import { beforeEach, describe, expect, it, vi } from 'vitest';

const host = vi.hoisted(() => ({
  flags: 0,
  payload: '',
  jwtReads: 0,
  flagReads: 0,
}));
vi.mock('spacetime:sys@2.0', () => ({
  moduleHooks: Symbol('moduleHooks'),
  identity: () => 1n,
  row_iter_bsatn_close: () => {},
  procedure_start_mut_tx: () => 0n,
  procedure_commit_mut_tx: () => {},
  procedure_abort_mut_tx: () => {},
  get_jwt_payload: () => {
    host.jwtReads++;
    return new TextEncoder().encode(host.payload);
  },
}));
vi.mock('spacetime:sys@2.1', () => ({}));
vi.mock('spacetime:sys@2.2', () => ({
  get_call_auth_flags: () => {
    host.flagReads++;
    return host.flags;
  },
}));

import { ReducerCtxImpl } from '../src/server/runtime';
import { ConnectionId } from '../src/lib/connection_id';
import { Identity } from '../src/lib/identity';
import { Timestamp } from '../src/lib/timestamp';
import { schema, exportContext, registerExport } from '../src/server/schema';
import { callProcedure } from '../src/server/procedures';
import { t } from '../src/lib/type_builders';
import { RawModuleDef } from '../src/lib/autogen/types';
import BinaryReader from '../src/lib/binary_reader';
import BinaryWriter from '../src/lib/binary_writer';

beforeEach(() => {
  Object.assign(host, { flags: 0, payload: '', jwtReads: 0, flagReads: 0 });
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
      host.flags = flags ^ 1;
      expect(host.flagReads).toBe(1);
      expect(ctx.senderAuth.isInternal).toBe(Boolean(flags));
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
    host.flags = 0;
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
    host.flags = 1;
    expect(firstAuth.isInternal).toBe(true);
    expect(ctx.senderAuth.isInternal).toBe(false);
    expect(ctx.senderAuth.hasJWT).toBe(false);
  });

  it('preserves procedure auth inside a transaction after the host flags change', () => {
    host.flags = 1;
    const module = schema({});
    const proc = module.procedure(t.unit(), ctx => {
      host.flags = 0;
      ctx.withTx(tx => {
        expect(tx.senderAuth).toBe(ctx.senderAuth);
        expect(tx.senderAuth.isInternal).toBe(true);
        expect(tx.connectionId).toBe(ctx.connectionId);
      });
      return {};
    });
    const inner = proc[exportContext]!;
    proc[registerExport](inner, 'procedure_auth');
    callProcedure(
      inner,
      0,
      new Identity(9n),
      new ConnectionId(8n),
      Timestamp.UNIX_EPOCH,
      new Uint8Array(),
      () => ({})
    );
    expect(host.flagReads).toBe(1);
  });
});

describe('V11 explicit function visibility', () => {
  it('serializes omission separately from explicit visibility and advertises hosted auth', () => {
    const module = schema({});
    const omitted = module.reducer(() => {});
    const explicitlyPublic = module.reducer({ visibility: 'public' }, () => {});
    const privateReducer = module.reducer({ visibility: 'private' }, () => {});
    const internalReducer = module.reducer(
      { visibility: 'internal' },
      () => {}
    );
    const inner = omitted[exportContext]!;
    for (const [name, reducer] of Object.entries({
      omitted,
      explicitlyPublic,
      privateReducer,
      internalReducer,
    })) {
      reducer[registerExport](inner, name);
    }
    // Being scheduled must not erase a public choice or manufacture an explicit
    // choice for the default. The host resolves the latter to Private.
    inner.moduleDef.schedules.push({
      sourceName: undefined,
      tableName: 'jobs',
      scheduleAtCol: 0,
      functionName: 'explicitlyPublic',
    });
    const raw = RawModuleDef.V11(inner.rawModuleDefV11());
    const writer = new BinaryWriter(128);
    RawModuleDef.serialize(writer, raw);
    expect(writer.getBuffer()[0]).toBe(3);
    const decoded = RawModuleDef.deserialize(
      new BinaryReader(writer.getBuffer())
    );
    const roundTrip = new BinaryWriter(128);
    RawModuleDef.serialize(roundTrip, decoded);
    expect(roundTrip.getBuffer()).toEqual(writer.getBuffer());
    expect(decoded.tag).toBe('V11');
    if (decoded.tag !== 'V11') throw new Error('Expected V11');
    const reducers = decoded.value.sections.find(
      section => section.tag === 'Reducers'
    );
    expect(
      reducers?.value.map(reducer => reducer.declaredVisibility?.tag)
    ).toEqual([undefined, 'ClientCallable', 'Private', 'Internal']);
    expect(
      inner.moduleDef.reducers.map(reducer => reducer.declaredVisibility?.tag)
    ).toEqual([undefined, 'ClientCallable', 'Private', 'Internal']);
    expect(inner.moduleDef.capabilities).toEqual(['hosted_auth_v1']);
  });

  it('retains procedure names and explicit visibility, including a visibility parameter', () => {
    const module = schema({});
    const proc = module.procedure(
      { name: 'public_name', visibility: 'internal' },
      t.unit(),
      () => ({})
    );
    const reducer = module.reducer({ visibility: t.string() }, () => {});
    const inner = proc[exportContext]!;
    proc[registerExport](inner, 'source_name');
    reducer[registerExport](inner, 'accept_visibility');
    expect(inner.moduleDef.procedures[0].sourceName).toBe('source_name');
    expect(inner.moduleDef.procedures[0].declaredVisibility?.tag).toBe(
      'Internal'
    );
    expect(inner.moduleDef.explicitNames.entries).toContainEqual({
      tag: 'Function',
      value: { sourceName: 'source_name', canonicalName: 'public_name' },
    });
    expect(inner.moduleDef.reducers[0].params.elements[0].name).toBe(
      'visibility'
    );
  });

  it('rejects externally callable lifecycle declarations', () => {
    const module = schema({});
    const invalid = module.init({ visibility: 'private' }, () => {});
    expect(() =>
      invalid[registerExport](invalid[exportContext]!, 'invalid_init')
    ).toThrow('Lifecycle reducers only support internal visibility');
    const valid = module.init({ visibility: 'internal' }, () => {});
    valid[registerExport](valid[exportContext]!, 'valid_init');
    expect(
      valid[exportContext]!.moduleDef.reducers[0].declaredVisibility?.tag
    ).toBe('Internal');
  });
});
