import { describe, expect, it } from 'vitest';
import { ConnectionId, Identity, TimeDuration, Timestamp, Uuid } from '../src';
import { ModuleContext, tablesToSchema } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { TableCacheImpl } from '../src/sdk/table_cache';
import { t } from '../src/lib/type_builders';
import { Range } from '../src/server/range';

// Signed columns get keys on both sides of zero.
const columns = [
  [
    'Identity',
    () => t.identity().index('btree'),
    (n: bigint) => new Identity(n),
    0n,
  ],
  [
    'ConnectionId',
    () => t.connectionId().index('btree'),
    (n: bigint) => new ConnectionId(n),
    0n,
  ],
  ['Uuid', () => t.uuid().index('btree'), (n: bigint) => new Uuid(n), 0n],
  [
    'Timestamp',
    () => t.timestamp().index('btree'),
    (n: bigint) => new Timestamp(n),
    -25n,
  ],
  [
    'TimeDuration',
    () => t.timeDuration().index('btree'),
    (n: bigint) => new TimeDuration(n),
    -25n,
  ],
] as const;

describe('table cache range bounds over wrapper column types', () => {
  it.each(columns)(
    'bounds a %s index by the wrapped integer',
    (_name, column, make, offset) => {
      const keys = [10n, 20n, 30n, 40n].map(n => n + offset);
      const rows = table(
        { name: 'rows' },
        { label: t.string(), term: column() }
      );
      const schema = tablesToSchema(new ModuleContext(), { rows });
      const cache: any = new TableCacheImpl<any, string>(
        schema.tables.rows as any
      );
      cache
        .applyOperations(
          keys.map((n, i) => ({
            type: 'insert' as const,
            rowId: `${i}`,
            row: { label: `${i}`, term: make(n) },
          })),
          {}
        )
        .forEach((callback: { cb: () => void }) => callback.cb());

      // The bounds are fresh instances, not the ones stored in the cache.
      const scan = cache.term.filter(
        new Range<any>(
          { tag: 'excluded', value: make(keys[0]) },
          { tag: 'included', value: make(keys[2]) }
        )
      );
      expect([...scan].map(row => row.label)).toEqual(['1', '2']);
    }
  );
});
