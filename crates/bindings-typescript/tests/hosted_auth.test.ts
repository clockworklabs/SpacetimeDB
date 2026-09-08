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
vi.mock('spacetime:sys@2.3', () => ({ env_get: () => null }));
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
import {
  AlgebraicType,
  FunctionVisibility,
  ProductType,
  RawModuleDef,
  RawModuleDefV10Section,
  RawReducerDefV10,
} from '../src/lib/autogen/types';
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

describe('V10 explicit function visibility', () => {
  it('preserves existing visibility tags and appends the new variants and capability section', () => {
    const legacyVisibility = t.enum('LegacyFunctionVisibility', {
      Private: t.unit(),
      ClientCallable: t.unit(),
    });
    const variants = [
      FunctionVisibility.Private,
      FunctionVisibility.ClientCallable,
      FunctionVisibility.Internal,
      FunctionVisibility.ExplicitClientCallable,
    ];
    for (const [tag, visibility] of variants.entries()) {
      const writer = new BinaryWriter(8);
      FunctionVisibility.serialize(writer, visibility);
      expect([...writer.getBuffer()]).toEqual([tag]);
      const reader = new BinaryReader(writer.getBuffer());
      if (tag < 2) {
        expect(legacyVisibility.deserialize(reader).tag).toBe(visibility.tag);
      }
    }
    const writer = new BinaryWriter(8);
    RawModuleDefV10Section.serialize(writer, {
      tag: 'Capabilities',
      value: [],
    });
    expect([...writer.getBuffer()]).toEqual([13, 0, 0, 0, 0]);
  });

  it('retains the V10 reducer field layout without an optional visibility wrapper', () => {
    const module = schema({});
    const reducer = module.reducer({ visibility: 'public' }, () => {});
    const inner = reducer[exportContext]!;
    reducer[registerExport](inner, 'public_reducer');
    const definition = inner.moduleDef.reducers[0];
    const writer = new BinaryWriter(128);
    RawReducerDefV10.serialize(writer, definition);
    const expected = new BinaryWriter(128);
    expected.writeString(definition.sourceName);
    ProductType.serialize(expected, definition.params);
    expected.writeByte(3);
    AlgebraicType.serialize(expected, definition.okReturnType);
    AlgebraicType.serialize(expected, definition.errReturnType);
    expect(writer.getBuffer()).toEqual(expected.getBuffer());
  });

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
    for (const name of [
      'omitted',
      'explicitlyPublic',
      'privateReducer',
      'internalReducer',
    ]) {
      inner.moduleDef.schedules.push({
        sourceName: undefined,
        tableName: `jobs_${name}`,
        scheduleAtCol: 0,
        functionName: name,
      });
    }
    const raw = RawModuleDef.V10(inner.rawModuleDefV10());
    const writer = new BinaryWriter(128);
    RawModuleDef.serialize(writer, raw);
    expect(writer.getBuffer()[0]).toBe(2);
    const decoded = RawModuleDef.deserialize(
      new BinaryReader(writer.getBuffer())
    );
    const roundTrip = new BinaryWriter(128);
    RawModuleDef.serialize(roundTrip, decoded);
    expect(roundTrip.getBuffer()).toEqual(writer.getBuffer());
    expect(decoded.tag).toBe('V10');
    if (decoded.tag !== 'V10') throw new Error('Expected V10');
    const reducers = decoded.value.sections.find(
      section => section.tag === 'Reducers'
    );
    expect(reducers?.value.map(reducer => reducer.visibility.tag)).toEqual([
      'ClientCallable',
      'ExplicitClientCallable',
      'Private',
      'Internal',
    ]);
    expect(
      inner.moduleDef.reducers.map(reducer => reducer.visibility.tag)
    ).toEqual([
      'ClientCallable',
      'ExplicitClientCallable',
      'Private',
      'Internal',
    ]);
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
    expect(inner.moduleDef.procedures[0].visibility.tag).toBe('Internal');
    expect(inner.moduleDef.explicitNames.entries).toContainEqual({
      tag: 'Function',
      value: { sourceName: 'source_name', canonicalName: 'public_name' },
    });
    expect(inner.moduleDef.reducers[0].params.elements[0].name).toBe(
      'visibility'
    );
  });

  it.each(['private', 'public'] as const)(
    'rejects explicit %s lifecycle declarations',
    visibility => {
      const module = schema({});
      const invalid = module.init({ visibility }, () => {});
      expect(() =>
        invalid[registerExport](invalid[exportContext]!, 'invalid_init')
      ).toThrow('Lifecycle reducers only support internal visibility');
    }
  );

  it.each([undefined, 'internal'] as const)(
    'preserves permitted lifecycle declaration %s for host event dispatch',
    visibility => {
      const module = schema({});
      const valid = module.init({ visibility }, () => {});
      valid[registerExport](valid[exportContext]!, 'valid_init');
      const inner = valid[exportContext]!;
      expect(inner.moduleDef.reducers[0].visibility.tag).toBe(
        visibility === undefined ? 'ClientCallable' : 'Internal'
      );
      expect(inner.moduleDef.lifeCycleReducers).toEqual([
        { lifecycleSpec: { tag: 'Init' }, functionName: 'valid_init' },
      ]);
    }
  );
});
