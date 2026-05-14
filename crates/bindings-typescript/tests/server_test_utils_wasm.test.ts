import { describe, expect, it } from 'vitest';

import { ConnectionId, TimeDuration } from '../src';
import { Range, schema, table, t } from '../src/server';
import {
  createModuleTestHarness,
  createProcedureTestHooks,
  TestAuth,
  TestClock,
} from '../src/server/test-utils';
describe('server test-utils real wasm runtime', () => {
  it('validates JWT auth through the harness datastore', () => {
    const { spacetime, moduleExports } = makeModule();
    const test = createModuleTestHarness(spacetime, moduleExports);
    const payload = JSON.stringify({ iss: 'issuer', sub: 'subject' });
    const connectionId = new ConnectionId(7n);

    let seen:
      | {
          sender: string;
          authIdentity: string;
          connectionId: string | undefined;
        }
      | undefined;

    test.withReducerTx(TestAuth.fromJwtPayload(payload, connectionId), ctx => {
      seen = {
        sender: ctx.sender.toHexString(),
        authIdentity: ctx.senderAuth.jwt?.identity.toHexString() ?? '',
        connectionId: ctx.connectionId?.toHexString(),
      };
    });

    expect(seen?.sender).toMatch(/^[0-9a-f]{64}$/);
    expect(seen?.authIdentity).toBe(seen?.sender);
    expect(seen?.connectionId).toBe(connectionId.toHexString());
  });

  it('accepts JWT payloads without iat and with matching hex_identity', () => {
    const basePayload = { iss: 'issuer', sub: 'subject' };
    const sender = senderForJwtPayload(basePayload);

    expect(sender).toMatch(/^[0-9a-f]{64}$/);
    expect(
      senderForJwtPayload({ ...basePayload, hex_identity: sender })
    ).toBe(sender);
  });

  it.each([
    ['missing iss', { sub: 'subject' }],
    ['missing sub', { iss: 'issuer' }],
    ['empty iss', { iss: '', sub: 'subject' }],
    ['empty sub', { iss: 'issuer', sub: '' }],
    ['non-string hex_identity', { iss: 'issuer', sub: 'subject', hex_identity: 1 }],
    [
      'mismatched hex_identity',
      { iss: 'issuer', sub: 'subject', hex_identity: '00'.repeat(32) },
    ],
  ])('rejects JWT payloads with %s', (_name, payload) => {
    const { spacetime, moduleExports } = makeModule();
    const test = createModuleTestHarness(spacetime, moduleExports);
    const connectionId = new ConnectionId(7n);

    expect(() =>
      test.withReducerTx(
        TestAuth.fromJwtPayload(JSON.stringify(payload), connectionId),
        () => {}
      )
    ).toThrow();
  });

  it('runs procedure sleep hooks and advances the test clock', () => {
    const { spacetime, moduleExports } = makeModule();
    const clock = new TestClock();
    const test = createModuleTestHarness(spacetime, moduleExports, { clock });
    const hooks = createProcedureTestHooks<typeof spacetime.schemaType>();
    const wakeTimes: bigint[] = [];

    hooks.onSleep((test, wakeTime) => {
      wakeTimes.push(wakeTime.microsSinceUnixEpoch);
      test.clock.advance(TimeDuration.fromMillis(25));
    });

    const ctx = test
      .procedureContextBuilder(TestAuth.internal())
      .hooks(hooks)
      .build();

    ctx.sleep(TimeDuration.fromMillis(10));

    expect(wakeTimes).toEqual([10_000n]);
    expect(clock.now().microsSinceUnixEpoch).toBe(25_000n);
  });

  it('advances the clock to the wake time when sleep hooks do not', () => {
    const { spacetime, moduleExports } = makeModule();
    const clock = new TestClock();
    const test = createModuleTestHarness(spacetime, moduleExports, { clock });
    const ctx = test.procedureContext(TestAuth.internal());

    ctx.sleep(TimeDuration.fromMillis(10));

    expect(clock.now().microsSinceUnixEpoch).toBe(10_000n);
  });

  it('commits procedure transactions against the real wasm backend', () => {
    const { spacetime, moduleExports } = makeModule();
    const test = createModuleTestHarness(spacetime, moduleExports);
    const ctx = test.procedureContext(TestAuth.internal());

    ctx.withTx(tx => {
      tx.db.person.insert({ id: 10, name: 'Committed' });
    });

    expect([...test.db.person.iter()]).toEqual([
      { id: 10, name: 'Committed' },
    ]);
  });

  it('aborts procedure transactions when the body throws', () => {
    const { spacetime, moduleExports } = makeModule();
    const test = createModuleTestHarness(spacetime, moduleExports);
    const ctx = test.procedureContext(TestAuth.internal());

    expect(() =>
      ctx.withTx(tx => {
        tx.db.person.insert({ id: 11, name: 'Rolled back' });
        throw new Error('abort');
      })
    ).toThrow('abort');

    expect(test.db.person.count()).toBe(0n);
  });

  it('runs afterTxCommit hooks that can interleave reducers', () => {
    const { spacetime, moduleExports, addPerson } = makeModule();
    const test = createModuleTestHarness(spacetime, moduleExports);
    const hooks = createProcedureTestHooks<typeof spacetime.schemaType>();

    hooks.afterTxCommit(test => {
      test.withReducerTx(TestAuth.internal(), ctx => {
        addPerson(ctx, { id: 13, name: 'Reducer' });
      });
    });

    const ctx = test
      .procedureContextBuilder(TestAuth.internal())
      .hooks(hooks)
      .build();

    ctx.withTx(tx => {
      tx.db.person.insert({ id: 12, name: 'Procedure' });
    });

    expect([...test.db.person.iter()]).toEqual([
      { id: 12, name: 'Procedure' },
      { id: 13, name: 'Reducer' },
    ]);
  });

  it('uses the configured HTTP responder and allows interleaving before returning', () => {
    const { spacetime, moduleExports, addPerson } = makeModule();
    const test = createModuleTestHarness(spacetime, moduleExports);
    const ctx = test
      .procedureContextBuilder(TestAuth.internal())
      .http((test, req, body) => {
        expect(req.uri).toBe('https://example.com/');
        expect(new TextDecoder().decode(body)).toBe('request');
        test.withReducerTx(TestAuth.internal(), ctx => {
          addPerson(ctx, { id: 14, name: 'HTTP interleave' });
        });
        return {
          body: new TextEncoder().encode('response'),
          code: 201,
          headers: { entries: [] },
          version: { tag: 'Http11' },
        };
      })
      .build();

    const response = ctx.http.fetch('https://example.com/', {
      method: 'POST',
      body: 'request',
    });

    expect(response.status).toBe(201);
    expect([...test.db.person.iter()]).toEqual([
      { id: 14, name: 'HTTP interleave' },
    ]);
  });

  it('throws a deterministic error when no HTTP responder is configured', () => {
    const { spacetime, moduleExports } = makeModule();
    const test = createModuleTestHarness(spacetime, moduleExports);
    const ctx = test.procedureContext(TestAuth.internal());

    expect(() => ctx.http.fetch('https://example.com/')).toThrow(
      'no test HTTP responder configured'
    );
  });

  it('can create multiple independent harnesses for the same module', () => {
    const { spacetime, moduleExports } = makeModule();
    const first = createModuleTestHarness(spacetime, moduleExports);
    const second = createModuleTestHarness(spacetime, moduleExports);

    first.db.person.insert({ id: 20, name: 'First' });
    second.db.person.insert({ id: 21, name: 'Second' });

    expect([...first.db.person.iter()]).toEqual([{ id: 20, name: 'First' }]);
    expect([...second.db.person.iter()]).toEqual([{ id: 21, name: 'Second' }]);
  });

  it('supports generated columns, index find/filter/update/delete, and range scans', () => {
    const { spacetime, moduleExports } = makeTableHandleModule();
    const test = createModuleTestHarness(spacetime, moduleExports);

    const alpha = test.db.item.insert({
      id: 0,
      name: 'Alpha',
      group: 'a',
      score: 10,
    });
    const beta = test.db.item.insert({
      id: 0,
      name: 'Beta',
      group: 'b',
      score: 20,
    });
    const gamma = test.db.item.insert({
      id: 0,
      name: 'Gamma',
      group: 'a',
      score: 30,
    });

    expect(alpha.id).not.toBe(0);
    expect(beta.id).not.toBe(alpha.id);
    expect(gamma.id).not.toBe(beta.id);
    expect(test.db.item.id.find(alpha.id)).toEqual(alpha);
    expect(test.db.item.name.find('Beta')).toEqual(beta);
    expect([...test.db.item.group.filter('a')].map(row => row.name).sort()).toEqual([
      'Alpha',
      'Gamma',
    ]);
    expect(
      [
        ...test.db.item.score.filter(
          new Range(
            { tag: 'included', value: 10 },
            { tag: 'included', value: 20 }
          )
        ),
      ].map(row => row.name)
    ).toEqual(['Alpha', 'Beta']);

    const updated = test.db.item.id.update({
      ...alpha,
      name: 'Alpha2',
      score: 15,
    });
    expect(updated).toEqual({ ...alpha, name: 'Alpha2', score: 15 });
    expect(test.db.item.name.find('Alpha')).toBeNull();
    expect(test.db.item.name.find('Alpha2')).toEqual(updated);

    expect(test.db.item.delete(updated)).toBe(true);
    expect(test.db.item.id.find(updated.id)).toBeNull();
    expect(test.db.item.name.delete('Beta')).toBe(true);
    expect(test.db.item.id.find(beta.id)).toBeNull();
    expect(test.db.item.group.delete('a')).toBe(1);
    expect(test.db.item.count()).toBe(0n);
  });

  it('supports table clear operations', () => {
    const { spacetime, moduleExports } = makeTableHandleModule();
    const test = createModuleTestHarness(spacetime, moduleExports);
    test.db.item.insert({ id: 0, name: 'Second', group: 'x', score: 2 });
    test.db.item.insert({ id: 0, name: 'Third', group: 'x', score: 3 });

    expect(test.db.item.clear()).toBe(2n);
    expect(test.db.item.count()).toBe(0n);
  });

  it('surfaces unique constraint errors from table handles', () => {
    const { spacetime, moduleExports } = makeTableHandleModule();
    const test = createModuleTestHarness(spacetime, moduleExports);

    test.db.item.insert({ id: 0, name: 'Duplicate', group: 'x', score: 1 });
    expect(() =>
      test.db.item.insert({
        id: 0,
        name: 'Duplicate',
        group: 'y',
        score: 2,
      })
    ).toThrow();
  });

  it('runs typed queries against committed wasm datastore state', () => {
    const { spacetime, moduleExports, addPerson } = makeQueryModule();
    const test = createModuleTestHarness(spacetime, moduleExports);
    const viewCtx = test.viewContext(TestAuth.internal());

    test.db.person.insert({ id: 1, name: 'Alice' });
    test.withReducerTx(TestAuth.internal(), ctx => {
      addPerson(ctx, { id: 2, name: 'Bob' });
    });

    expect(test.runQuery(viewCtx.from.person)).toEqual([
      { id: 1, name: 'Alice' },
      { id: 2, name: 'Bob' },
    ]);
    expect(
      test.runQuery(viewCtx.from.person.where(row => row.name.eq('Bob')))
    ).toEqual([{ id: 2, name: 'Bob' }]);
  });

  it('runs typed queries returned by views', () => {
    const { spacetime, moduleExports, peopleNamedAlice } = makeQueryModule();
    const test = createModuleTestHarness(spacetime, moduleExports);

    test.db.person.insert({ id: 1, name: 'Alice' });
    test.db.person.insert({ id: 2, name: 'Bob' });

    expect(test.runQuery(peopleNamedAlice(test.viewContext(TestAuth.internal()), {}))).toEqual([
      { id: 1, name: 'Alice' },
    ]);
  });

  it('runs semijoin queries through the wasm datastore', () => {
    const { spacetime, moduleExports } = makeQueryModule();
    const test = createModuleTestHarness(spacetime, moduleExports);
    const viewCtx = test.viewContext(TestAuth.internal());

    test.db.person.insert({ id: 1, name: 'Alice' });
    test.db.person.insert({ id: 2, name: 'Bob' });
    test.db.pet.insert({ id: 10, ownerId: 2, name: 'Biscuit' });

    expect(
      test.runQuery(
        viewCtx.from.person.leftSemijoin(viewCtx.from.pet, (person, pet) =>
          person.id.eq(pet.ownerId)
        )
      )
    ).toEqual([{ id: 2, name: 'Bob' }]);
  });
});

function makeModule() {
  const person = table(
    { name: 'person', public: true },
    {
      id: t.u32(),
      name: t.string(),
    }
  );
  const spacetime = schema({ person });
  const addPerson = spacetime.reducer(
    { id: t.u32(), name: t.string() },
    (ctx, row) => {
      ctx.db.person.insert(row);
    }
  );

  return { spacetime, moduleExports: { addPerson }, addPerson };
}

function makeTableHandleModule() {
  const item = table(
    { name: 'item', public: true },
    {
      id: t.u32().primaryKey().autoInc(),
      name: t.string().unique(),
      group: t.string().index('hash'),
      score: t.u32().index('btree'),
    }
  );
  const spacetime = schema({ item });

  return { spacetime, moduleExports: {} };
}

function makeQueryModule() {
  const person = table(
    { name: 'person', public: true },
    {
      id: t.u32().primaryKey(),
      name: t.string(),
    }
  );
  const pet = table(
    { name: 'pet', public: true },
    {
      id: t.u32().primaryKey(),
      ownerId: t.u32().index('btree'),
      name: t.string(),
    }
  );
  const spacetime = schema({ person, pet });
  const addPerson = spacetime.reducer(
    { id: t.u32(), name: t.string() },
    (ctx, row) => {
      ctx.db.person.insert(row);
    }
  );
  const peopleNamedAlice = spacetime.view(
    { name: 'people_named_alice', public: true },
    t.array(person.rowType),
    ctx => ctx.from.person.where(row => row.name.eq('Alice'))
  );

  return {
    spacetime,
    moduleExports: { addPerson, peopleNamedAlice },
    addPerson,
    peopleNamedAlice,
  };
}

function senderForJwtPayload(payload: Record<string, unknown>): string {
  const { spacetime, moduleExports } = makeModule();
  const test = createModuleTestHarness(spacetime, moduleExports);
  const connectionId = new ConnectionId(7n);
  let sender: string | undefined;

  test.withReducerTx(
    TestAuth.fromJwtPayload(JSON.stringify(payload), connectionId),
    ctx => {
      sender = ctx.sender.toHexString();
    }
  );

  return sender!;
}
