import { beforeAll, describe, expect, it, vi } from 'vitest';
import BinaryWriter from '../src/lib/binary_writer';
import { HttpRequest } from '../src/lib/autogen/types';
import { Identity } from '../src/lib/identity';

const host = vi.hoisted(() => {
  const databaseIdentity = 0x1111n;
  return {
    databaseIdentity,
    // Every connection has this JWT payload.
    mockSys: async (importOriginal: () => Promise<object>) => ({
      ...(await importOriginal()),
      identity: () => databaseIdentity,
      get_jwt_payload: () => new TextEncoder().encode('{"sub":"client"}'),
    }),
  };
});

vi.mock('spacetime:sys@2.0', host.mockSys);
vi.mock('spacetime:sys@2.1', host.mockSys);

describe('ctx.senderAuth', () => {
  let schema: typeof import('../src/server/schema').schema;
  let SyncResponse: typeof import('../src/server/http_handlers').SyncResponse;
  let moduleHooks: symbol;

  beforeAll(async () => {
    ({ schema } = await import('../src/server/schema'));
    ({ SyncResponse } = await import('../src/server/http_handlers'));
    ({ moduleHooks } = (await import('spacetime:sys@2.0')) as any);
  });

  function reducerSenderAuth(sender: bigint, connectionId: bigint) {
    const spacetimedb = schema({});
    let senderAuth: any;
    const myReducer = spacetimedb.reducer(ctx => {
      senderAuth = ctx.senderAuth;
    });
    const hooks = (spacetimedb as any)[moduleHooks]({ myReducer });
    hooks.__call_reducer__(
      0,
      sender,
      connectionId,
      0n,
      new DataView(new ArrayBuffer(0))
    );
    return senderAuth;
  }

  it('is internal, with no JWT, when the sender is the database', () => {
    const auth = reducerSenderAuth(host.databaseIdentity, 1n);
    expect(auth.isInternal).toBe(true);
    expect(auth.hasJWT).toBe(false);
    expect(auth.jwt).toBeNull();
  });

  it('is not internal when the sender is another identity', () => {
    const auth = reducerSenderAuth(0x2222n, 1n);
    expect(auth.isInternal).toBe(false);
    expect(auth.jwt.subject).toBe('client');
  });

  it('has no JWT without a connection id, as in init', () => {
    const auth = reducerSenderAuth(0x2222n, 0n);
    expect(auth.isInternal).toBe(false);
    expect(auth.hasJWT).toBe(false);
  });

  it('is the HTTP request sender and its JWT in handler transactions', () => {
    const spacetimedb = schema({});
    let handlerSender: any;
    let tx: any;
    const handler = spacetimedb.httpHandler(ctx => {
      handlerSender = ctx.sender;
      ctx.withTx(txCtx => {
        tx = { sender: txCtx.sender, senderAuth: txCtx.senderAuth };
      });
      return new SyncResponse('ok');
    });
    const hooks = (spacetimedb as any)[moduleHooks]({ handler });
    const request = new BinaryWriter(64);
    HttpRequest.serialize(request, {
      method: { tag: 'Get' },
      headers: { entries: [] },
      timeout: undefined,
      uri: '/',
      version: { tag: 'Http11' },
    });
    hooks.__call_http_handler__(
      0,
      0n,
      request.getBuffer(),
      new Uint8Array(),
      0x3333n,
      new TextEncoder().encode('{"sub":"request"}')
    );

    expect(handlerSender.isEqual(new Identity(0x3333n))).toBe(true);
    expect(tx.sender.isEqual(handlerSender)).toBe(true);
    expect(tx.senderAuth.isInternal).toBe(false);
    expect(tx.senderAuth.jwt.subject).toBe('request');
  });
});
