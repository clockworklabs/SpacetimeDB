import { describe, expect, it } from 'vitest';
import { Identity } from 'spacetimedb';
import {
  createModuleTestHarness,
  TestAuth,
} from 'spacetimedb/server/test-utils';

import spacetime, * as moduleExports from './index';

describe('chat module unit tests', () => {
  it('can test connect, set_name, and send_message reducers directly', () => {
    const test = createModuleTestHarness(spacetime, moduleExports);
    const alice = new Identity(1n);

    test.withReducerTx(TestAuth.internal(alice), ctx => {
      moduleExports.onConnect(ctx);
      moduleExports.set_name(ctx, { name: 'Alice' });
      moduleExports.send_message(ctx, { text: 'hello' });
    });

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
    const alice = new Identity(1n);
    const ctx = test.procedureContext(TestAuth.internal(alice));

    ctx.withTx(tx => {
      moduleExports.onConnect(tx);
      moduleExports.set_name(tx, { name: 'Alice' });
    });

    expect(test.db.user.identity.find(alice)?.name).toBe('Alice');
  });
});
