import { describe, expect, it } from 'vitest';
import { ConnectionId, Timestamp, type Identity } from 'spacetimedb';
import {
  createModuleTestHarness,
  TestAuth,
} from 'spacetimedb/server/test-utils';

import spacetime, * as moduleExports from './index';

function testAuth(subject: string, connectionId: bigint) {
  return TestAuth.fromJwtPayload(
    JSON.stringify({ iss: 'chat-template-test', sub: subject }),
    new ConnectionId(connectionId)
  );
}

describe('chat module unit tests', () => {
  it('preserves a user across disconnect and reconnect', () => {
    const test = createModuleTestHarness(spacetime, moduleExports);
    const auth = testAuth('alice', 1n);
    let alice: Identity | undefined;

    test.withReducerTx(auth, ctx => {
      alice = ctx.sender;
      moduleExports.onConnect(ctx, {});
    });

    expect(alice).toBeDefined();
    if (!alice) throw new Error('expected Alice to be initialized');
    expect(test.db.user.identity.find(alice)).toMatchObject({
      name: undefined,
      online: true,
    });

    test.withReducerTx(auth, ctx => {
      moduleExports.set_name(ctx, { name: 'Alice' });
      moduleExports.onDisconnect(ctx, {});
    });
    expect(test.db.user.identity.find(alice)).toMatchObject({
      name: 'Alice',
      online: false,
    });

    test.withReducerTx(auth, ctx => moduleExports.onConnect(ctx, {}));
    expect(test.db.user.identity.find(alice)).toMatchObject({
      name: 'Alice',
      online: true,
    });
  });

  it('rejects invalid names without modifying users', () => {
    const test = createModuleTestHarness(spacetime, moduleExports);
    const auth = testAuth('alice', 1n);
    let alice: Identity | undefined;

    expect(() =>
      test.withReducerTx(auth, ctx => {
        alice = ctx.sender;
        moduleExports.set_name(ctx, { name: 'Alice' });
      })
    ).toThrow('Cannot set name for unknown user');

    expect(alice).toBeDefined();
    if (!alice) throw new Error('expected Alice to be initialized');
    expect(test.db.user.identity.find(alice)).toBeNull();

    test.withReducerTx(auth, ctx => {
      moduleExports.onConnect(ctx, {});
      moduleExports.set_name(ctx, { name: 'Alice' });
    });

    expect(() =>
      test.withReducerTx(auth, ctx => moduleExports.set_name(ctx, { name: '' }))
    ).toThrow('Names must not be empty');
    expect(test.db.user.identity.find(alice)?.name).toBe('Alice');
  });

  it('stores messages with the sender and current timestamp', () => {
    const now = new Timestamp(1_234_567n);
    const test = createModuleTestHarness(spacetime, moduleExports);
    const auth = testAuth('alice', 1n);
    let alice: Identity | undefined;
    test.clock.set(now);

    test.withReducerTx(auth, ctx => {
      alice = ctx.sender;
      moduleExports.onConnect(ctx, {});
      moduleExports.send_message(ctx, { text: 'hello' });
    });

    expect(alice).toBeDefined();
    if (!alice) throw new Error('expected Alice to be initialized');
    const messages = [...test.db.message.iter()];
    expect(messages).toHaveLength(1);
    expect(messages[0]?.sender.isEqual(alice)).toBe(true);
    expect(messages[0]?.text).toBe('hello');
    expect(messages[0]?.sent.microsSinceUnixEpoch).toBe(
      now.microsSinceUnixEpoch
    );

    expect(() =>
      test.withReducerTx(auth, ctx =>
        moduleExports.send_message(ctx, { text: '' })
      )
    ).toThrow('Messages must not be empty');
    expect([...test.db.message.iter()]).toHaveLength(1);
  });
});
