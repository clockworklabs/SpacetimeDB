import { describe, expect, it } from 'vitest';
import { ConnectionId, Identity, TimeDuration, Timestamp, Uuid } from '../src';
import { ModuleContext, tablesToSchema } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { TableCacheImpl } from '../src/sdk/table_cache';
import { t } from '../src/lib/type_builders';
import { Range } from '../src/server/range';

/**
 * `Identity`, `ConnectionId`, `Timestamp`, `TimeDuration` and `Uuid` are
 * indexable column types, so `index.filter(new Range(...))` over a column of
 * one of those types is a type-valid client-cache query. Each is a SATS
 * one-element product wrapping a single integer, and each is an object in JS,
 * so evaluating range bounds with bare `===` / `<` compares object identity
 * and `toString` coercions instead of the wrapped integer.
 *
 * The host orders these columns by the integer they wrap — `AlgebraicValue`'s
 * derived `Ord` descends through the one-element product — so the client cache
 * must order them the same way.
 */

const eventsCache = () => {
  const events = table(
    { name: 'events' },
    {
      label: t.string(),
      owner: t.identity().index('btree'),
      connection: t.connectionId().index('btree'),
      at: t.timestamp().index('btree'),
      ttl: t.timeDuration().index('btree'),
      traceId: t.uuid().index('btree'),
    }
  );
  const schema = tablesToSchema(new ModuleContext(), { events });
  return new TableCacheImpl<any, string>(schema.tables.events as any);
};

/**
 * `micros` drives the signed `Timestamp`/`TimeDuration` columns; `magnitude`
 * drives the unsigned `Identity`/`ConnectionId`/`Uuid` columns. Tests vary
 * whichever one the index under test is built on.
 */
const row = (label: string, micros: bigint, magnitude: bigint = 0n) => ({
  label,
  owner: new Identity(magnitude),
  connection: new ConnectionId(magnitude),
  at: new Timestamp(micros),
  ttl: new TimeDuration(micros),
  traceId: new Uuid(magnitude),
});

const insert = (cache: TableCacheImpl<any, string>, rows: any[]) => {
  const callbacks = cache.applyOperations(
    rows.map(r => ({ type: 'insert' as const, rowId: r.label, row: r })),
    {}
  );
  callbacks.forEach(cb => cb.cb());
};

const labels = (rows: Iterable<any>) => [...rows].map(r => r.label);

describe('table cache range bounds over wrapper column types', () => {
  it('bounds an Identity index by the wrapped u256', () => {
    const cache = eventsCache();
    insert(cache, [
      row('a', 0n, 10n),
      row('b', 0n, 20n),
      row('c', 0n, 30n),
      row('d', 0n, 40n),
    ]);

    const scan = (cache as any).owner.filter(
      new Range<Identity>(
        { tag: 'excluded', value: new Identity(10n) },
        { tag: 'included', value: new Identity(30n) }
      )
    );
    expect(labels(scan)).toEqual(['b', 'c']);
  });

  it('bounds a ConnectionId index by the wrapped u128', () => {
    const cache = eventsCache();
    insert(cache, [row('a', 0n, 10n), row('b', 0n, 20n), row('c', 0n, 30n)]);

    const scan = (cache as any).connection.filter(
      new Range<ConnectionId>(
        { tag: 'excluded', value: new ConnectionId(10n) },
        { tag: 'excluded', value: new ConnectionId(30n) }
      )
    );
    expect(labels(scan)).toEqual(['b']);
  });

  it('bounds a Uuid index by the wrapped u128', () => {
    const cache = eventsCache();
    const first = Uuid.parse('01888d6e-5c00-7000-8000-000000000000');
    const second = Uuid.parse('01999d6e-5c00-7000-8000-000000000000');
    const third = Uuid.parse('01aa8d6e-5c00-7000-8000-000000000000');
    insert(cache, [
      row('first', 0n, first.asBigInt()),
      row('second', 0n, second.asBigInt()),
      row('third', 0n, third.asBigInt()),
    ]);

    // A distinct `Uuid` instance with the same value must bound the scan the
    // same way the instance stored in the cache would.
    const scan = (cache as any).traceId.filter(
      new Range<Uuid>(
        { tag: 'included', value: Uuid.parse(second.toString()) },
        { tag: 'included', value: Uuid.parse(third.toString()) }
      )
    );
    expect(labels(scan)).toEqual(['second', 'third']);
  });

  it('bounds a Timestamp index by the wrapped micros', () => {
    const cache = eventsCache();
    insert(cache, [row('a', 10n), row('b', 20n), row('c', 30n)]);

    const scan = (cache as any).at.filter(
      new Range<Timestamp>(
        { tag: 'included', value: new Timestamp(20n) },
        { tag: 'unbounded' }
      )
    );
    expect(labels(scan)).toEqual(['b', 'c']);
  });

  it('orders pre-epoch Timestamps below the epoch', () => {
    const cache = eventsCache();
    insert(cache, [row('before', -10n), row('epoch', 0n), row('after', 10n)]);

    const scan = (cache as any).at.filter(
      new Range<Timestamp>(
        { tag: 'unbounded' },
        { tag: 'excluded', value: Timestamp.UNIX_EPOCH }
      )
    );
    expect(labels(scan)).toEqual(['before']);
  });

  it('bounds a TimeDuration index by the wrapped micros', () => {
    const cache = eventsCache();
    insert(cache, [row('a', -5n), row('b', 0n), row('c', 5n)]);

    const scan = (cache as any).ttl.filter(
      new Range<TimeDuration>(
        { tag: 'included', value: new TimeDuration(0n) },
        { tag: 'unbounded' }
      )
    );
    expect(labels(scan)).toEqual(['b', 'c']);
  });

  it('still matches bare-value (equality) terms on wrapper columns', () => {
    const cache = eventsCache();
    insert(cache, [row('a', 10n, 10n), row('b', 20n, 20n)]);

    expect(labels((cache as any).at.filter(new Timestamp(20n)))).toEqual(['b']);
    expect(labels((cache as any).owner.filter(new Identity(10n)))).toEqual([
      'a',
    ]);
  });

  it('leaves primitive-column ranges alone', () => {
    const numbers = table(
      { name: 'numbers' },
      {
        label: t.string(),
        score: t.i32().index('btree'),
        big: t.u64().index('btree'),
      }
    );
    const schema = tablesToSchema(new ModuleContext(), { numbers });
    const cache = new TableCacheImpl<any, string>(schema.tables.numbers as any);
    insert(cache, [
      { label: 'a', score: -5, big: 1n },
      { label: 'b', score: 0, big: 2n },
      { label: 'c', score: 5, big: 3n },
    ]);

    expect(
      labels(
        (cache as any).score.filter(
          new Range<number>(
            { tag: 'included', value: -5 },
            { tag: 'excluded', value: 5 }
          )
        )
      )
    ).toEqual(['a', 'b']);
    expect(
      labels(
        (cache as any).big.filter(
          new Range<bigint>(
            { tag: 'excluded', value: 1n },
            { tag: 'unbounded' }
          )
        )
      )
    ).toEqual(['b', 'c']);
  });
});
