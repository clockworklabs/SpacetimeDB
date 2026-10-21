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
import { Range, isRange } from '../src/server/range';
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

function singleColumnView() {
  const ctx = new ModuleContext();
  const item = table(
    {
      name: 'item',
      indexes: [
        {
          accessor: 'by_n',
          algorithm: 'btree',
          columns: ['n'] as const,
        },
      ] as const,
    },
    {
      id: t.u64().primaryKey().autoInc(),
      n: t.u32(),
    }
  );

  const rawTableDef = item.tableDef(ctx, 'item');
  const view = makeTableView(ctx.typespace, rawTableDef) as any;
  return view.by_n;
}

// Simulated cross-bundle Range class: separate constructor identity, branded via Symbol.for
class CrossBundleRange<T> {
  #from: any;
  #to: any;
  constructor(from?: any, to?: any) {
    this.#from = from ?? { tag: 'unbounded' };
    this.#to = to ?? { tag: 'unbounded' };
  }
  public get [Symbol.for('spacetimedb.Range')](): true {
    return true;
  }
  public get from() {
    return this.#from;
  }
  public get to() {
    return this.#to;
  }
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

describe('cross-bundle Range compatibility (#6299)', () => {
  it('isRange correctly identifies native and cross-bundle Range instances', () => {
    const native = new Range({ tag: 'unbounded' }, { tag: 'unbounded' });
    const foreign = new CrossBundleRange(
      { tag: 'included', value: 2 },
      { tag: 'unbounded' }
    );
    const duckTyped = {
      constructor: { name: 'Range' },
      from: { tag: 'unbounded' },
      to: { tag: 'unbounded' },
    };

    expect(isRange(native)).toBe(true);
    expect(isRange(foreign)).toBe(true);
    expect(isRange(duckTyped)).toBe(true);
    expect(Range.isRange(native)).toBe(true);
    expect(Range.isRange(foreign)).toBe(true);

    expect(isRange(null)).toBe(false);
    expect(isRange(undefined)).toBe(false);
    expect(isRange(123)).toBe(false);
    expect(isRange('Range')).toBe(false);
    expect(isRange({ from: 1, to: 2 })).toBe(false);
  });

  it('filters single-column btree index with cross-bundle Range', () => {
    const index = singleColumnView();
    const range = new CrossBundleRange(
      { tag: 'included', value: 2 },
      { tag: 'unbounded' }
    );
    expect(() => [...index.filter(range)]).not.toThrow();
    expect([...index.filter(range)]).toEqual([]);
  });

  it('deletes from single-column btree index with cross-bundle Range', () => {
    const index = singleColumnView();
    const range = new CrossBundleRange(
      { tag: 'included', value: 2 },
      { tag: 'unbounded' }
    );
    expect(() => index.delete(range)).not.toThrow();
    expect(index.delete(range)).toBe(0);
  });

  it('filters multi-column btree index with cross-bundle Range', () => {
    const index = tallyCountView();
    const range = new CrossBundleRange(
      { tag: 'included', value: 10n },
      { tag: 'included', value: 20n }
    );
    expect(() => [...index.filter([1n, range])]).not.toThrow();
    expect([...index.filter([1n, range])]).toEqual([]);
  });

  it('deletes from multi-column btree index with cross-bundle Range', () => {
    const index = tallyCountView();
    const range = new CrossBundleRange(
      { tag: 'included', value: 10n },
      { tag: 'included', value: 20n }
    );
    expect(() => index.delete([1n, range])).not.toThrow();
    expect(index.delete([1n, range])).toBe(0);
  });
});
