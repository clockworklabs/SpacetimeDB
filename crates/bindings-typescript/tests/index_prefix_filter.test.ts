import { describe, expect, it, vi } from 'vitest';

// `runtime.ts` and `procedures.ts` form an import cycle (procedures extends
// runtime's ReducerCtxImpl). Vitest's loader evaluates the cycle in an order
// that leaves the base class undefined. runtime only needs `callProcedure`, and
// not on the table-view path under test, so stub procedures to break the cycle.
vi.mock('../src/server/procedures', () => ({
  callProcedure: () => {
    throw new Error('callProcedure is not stubbed for this test');
  },
}));

import { ModuleContext } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { Range } from '../src/server/range';
import { makeTableView } from '../src/server/runtime';
import { t } from '../src/lib/type_builders';

/**
 * Regression test for clockworklabs/SpacetimeDB#5407:
 *
 *   Calling `.filter()` / `.delete()` on a multi-column btree index with a
 *   single bare scalar (the documented one-column prefix scan) used to panic
 *   with `TypeError: serializeTerm is not a function` inside `serializeRange`,
 *   because a bare scalar has no `.length`, so `prefix_elems` and the serializer
 *   index both became `NaN`.
 *
 * `filter(1n)` is the only *type-valid* way to express a one-column prefix on a
 * `[u64, string]` index, so it must work; the full two-column key takes the
 * separate point-scan branch. The mocked host iterator yields no rows, so a
 * successful scan deserializes to an empty result rather than crashing.
 */
function tallyView() {
  const ctx = new ModuleContext();
  const tally = table(
    {
      name: 'tally',
      indexes: [
        {
          accessor: 'by_board_def',
          algorithm: 'btree',
          columns: ['boardId', 'defId'] as const,
        },
      ] as const,
    },
    {
      id: t.u64().primaryKey().autoInc(),
      boardId: t.u64(),
      defId: t.string(),
      count: t.u64(),
    }
  );

  const rawTableDef = tally.tableDef(ctx, 'tally');
  const view = makeTableView(ctx.typespace, rawTableDef) as any;
  return view.by_board_def;
}

function tallyCountView() {
  const ctx = new ModuleContext();
  const tally = table(
    {
      name: 'tally',
      indexes: [
        {
          accessor: 'by_board_count',
          algorithm: 'btree',
          columns: ['boardId', 'count'] as const,
        },
      ] as const,
    },
    {
      id: t.u64().primaryKey().autoInc(),
      boardId: t.u64(),
      defId: t.string(),
      count: t.u64(),
    }
  );

  const rawTableDef = tally.tableDef(ctx, 'tally');
  const view = makeTableView(ctx.typespace, rawTableDef) as any;
  return view.by_board_count;
}

describe('multi-column index one-column prefix scan', () => {
  it('full two-column key works (point scan)', () => {
    const index = tallyView();
    expect(() => [...index.filter([1n, 'regolith'])]).not.toThrow();
    expect([...index.filter([1n, 'regolith'])]).toEqual([]);
  });

  it('bare-scalar one-column prefix works (range scan)', () => {
    const index = tallyView();
    // `filter(1n)` is the documented one-column prefix form and the only
    // type-valid way to express it; it must scan, not throw.
    expect(() => [...index.filter(1n)]).not.toThrow();
    expect([...index.filter(1n)]).toEqual([]);
  });

  it('bare Range on the first column works (range scan)', () => {
    const index = tallyView();
    const range = new Range<bigint>(
      { tag: 'included', value: 1n },
      { tag: 'excluded', value: 5n }
    );
    expect(() => [...index.filter(range)]).not.toThrow();
    expect([...index.filter(range)]).toEqual([]);
  });

  it('full key with Range in final column works (range scan)', () => {
    const index = tallyCountView();
    const range = new Range<bigint>(
      { tag: 'included', value: 10n },
      { tag: 'included', value: 20n }
    );

    expect(() => [...index.filter([1n, range])]).not.toThrow();
    expect([...index.filter([1n, range])]).toEqual([]);
  });

  it('delete() accepts a bare-scalar one-column prefix', () => {
    const index = tallyView();
    expect(() => index.delete(1n)).not.toThrow();
    expect(index.delete(1n)).toBe(0);
  });

  it('delete() accepts a full key with Range in final column', () => {
    const index = tallyCountView();
    const range = new Range<bigint>(
      { tag: 'included', value: 10n },
      { tag: 'included', value: 20n }
    );

    expect(() => index.delete([1n, range])).not.toThrow();
    expect(index.delete([1n, range])).toBe(0);
  });
});

/**
 * `spacetimedb/server` and `spacetimedb/server/test-utils` are bundled
 * separately, so a module under unit test builds its `Range` from one copy of
 * the class while the harness's runtime checks `instanceof` against another.
 * The range used to be taken for a scalar key and the scan failed with
 * `TypeError: Cannot convert undefined to a BigInt`.
 */
describe('a Range from another copy of the package', () => {
  // A query string makes Vitest load the module a second time.
  const otherCopy = (): Promise<typeof import('../src/server/range')> =>
    // @ts-expect-error the query string is not part of the module's path
    import('../src/server/range?other-copy');

  it('is a different class that still passes instanceof', async () => {
    const { Range: OtherRange } = await otherCopy();
    expect(OtherRange).not.toBe(Range);
    expect(new OtherRange() instanceof Range).toBe(true);
    expect(new Range() instanceof OtherRange).toBe(true);
    expect({ from: 1, to: 2 } instanceof Range).toBe(false);
  });

  it('works in the final column of a full key (range scan)', async () => {
    const { Range: OtherRange } = await otherCopy();
    const index = tallyCountView();
    const range = new OtherRange<bigint>(
      { tag: 'included', value: 10n },
      { tag: 'included', value: 20n }
    );

    expect([...index.filter([1n, range])]).toEqual([]);
    expect(index.delete([1n, range])).toBe(0);
  });

  it('works bare on the first column (range scan)', async () => {
    const { Range: OtherRange } = await otherCopy();
    const index = tallyView();
    const range = new OtherRange<bigint>(
      { tag: 'included', value: 1n },
      { tag: 'excluded', value: 5n }
    );

    expect([...index.filter(range)]).toEqual([]);
  });
});
