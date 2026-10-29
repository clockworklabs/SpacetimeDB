// Unit tests run the module against an in-memory database, with no server.
// See https://spacetimedb.com/docs/how-to/unit-testing
import { expect, it } from 'vitest';
import { ConnectionId } from 'spacetimedb';
import {
  createModuleTestHarness,
  TestAuth,
} from 'spacetimedb/server/test-utils';
import spacetimedb, * as moduleExports from './index';

function clientAuth() {
  return TestAuth.fromJwtPayload(
    JSON.stringify({ iss: 'test-issuer', sub: 'alice' }),
    new ConnectionId(1n)
  );
}

it('adds a person', () => {
  const test = createModuleTestHarness(spacetimedb, moduleExports);

  test.withReducerTx(clientAuth(), ctx => {
    moduleExports.add(ctx, { name: 'Alice' });
  });

  expect([...test.db.person.iter()].map(person => person.name)).toEqual([
    'Alice',
  ]);
});

it('rolls back all writes when the transaction throws', () => {
  const test = createModuleTestHarness(spacetimedb, moduleExports);

  expect(() =>
    test.withReducerTx(clientAuth(), ctx => {
      moduleExports.add(ctx, { name: 'Alice' });
      throw new Error('Something went wrong');
    })
  ).toThrow('Something went wrong');

  expect(test.db.person.count()).toBe(0n);
});
