import { describe, expect, it } from 'vitest';
import { ConnectionId, Identity } from 'spacetimedb';
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
  it('can test connect, set_name, and send_message reducers directly', () => {
    const test = createModuleTestHarness(spacetime, moduleExports);
    let alice: Identity | undefined;

    test.withReducerTx(testAuth('alice', 1n), ctx => {
      alice = ctx.sender;
      moduleExports.onConnect(ctx);
      moduleExports.set_name(ctx, { name: 'Alice' });
      moduleExports.send_message(ctx, { text: 'hello' });
    });

    expect(alice).toBeDefined();
    const user = test.db.user.identity.find(alice);
    expect(user?.identity.toHexString()).toBe(alice.toHexString());
    expect(user?.name).toBe('Alice');
    expect(user?.online).toBe(true);
    expect([...test.db.message.iter()].map(message => message.text)).toEqual([
      'hello',
    ]);
  });

  it('can use a procedure context transaction in tests', () => {
    const test = createModuleTestHarness(spacetime, moduleExports);
    const ctx = test.procedureContext(testAuth('alice', 1n));
    let alice: Identity | undefined;

    ctx.withTx(tx => {
      alice = tx.sender;
      moduleExports.onConnect(tx);
      moduleExports.set_name(tx, { name: 'Alice' });
    });

    expect(alice).toBeDefined();
    expect(test.db.user.identity.find(alice)?.name).toBe('Alice');
  });

  it('can run typed queries against committed test state', () => {
    const test = createModuleTestHarness(spacetime, moduleExports);
    const aliceAuth = testAuth('alice', 1n);
    const bobAuth = testAuth('bob', 2n);

    test.withReducerTx(aliceAuth, ctx => {
      moduleExports.onConnect(ctx);
      moduleExports.send_message(ctx, { text: 'hello from alice' });
    });
    test.withReducerTx(bobAuth, ctx => {
      moduleExports.onConnect(ctx);
      moduleExports.send_message(ctx, { text: 'hello from bob' });
    });

    const viewCtx = test.viewContext(aliceAuth);
    const aliceMessages = test.runQuery(
      viewCtx.from.message.where(message => message.text.eq('hello from alice'))
    );

    expect(aliceMessages.map(message => message.text)).toEqual([
      'hello from alice',
    ]);
  });
});
