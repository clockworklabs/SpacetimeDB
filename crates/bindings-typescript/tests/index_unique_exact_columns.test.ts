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

// A host stub that can actually hand rows back, so a scan can be observed
// returning more than one row. `runtime.ts` builds its `sys` object by
// spreading both syscall modules at import time, so both have to be mocked.
const host = vi.hoisted(() => ({
  pointScans: 0,
  rangeScans: 0,
  /** BSATN for the rows the next iterator yields, concatenated. */
  rows: new Uint8Array(),
  exhausted: false,
  reset(rows: Uint8Array) {
    host.pointScans = 0;
    host.rangeScans = 0;
    host.rows = rows;
    host.exhausted = false;
  },
  overrides: {
    datastore_index_scan_range_bsatn: (): number => {
      host.rangeScans += 1;
      return 1;
    },
    datastore_index_scan_point_bsatn: (): number => {
      host.pointScans += 1;
      return 1;
    },
    // `> 0` means "yielded rows, more to come", `< 0` means "yielded rows and
    // is now exhausted", `0` means "empty". One chunk holds every row here.
    row_iter_bsatn_advance: (_iter: number, buffer: ArrayBuffer): number => {
      if (host.exhausted) return 0;
      host.exhausted = true;
      new Uint8Array(buffer).set(host.rows);
      return -host.rows.length;
    },
  },
}));

vi.mock('spacetime:sys@2.0', async importOriginal => ({
  ...(await importOriginal<object>()),
  ...host.overrides,
}));
vi.mock('spacetime:sys@2.1', async importOriginal => ({
  ...(await importOriginal<object>()),
  ...host.overrides,
}));

import { AlgebraicType } from '../src/lib/algebraic_type';
import BinaryWriter from '../src/lib/binary_writer';
import type { ConstraintOpts } from '../src/lib/constraints';
import { ModuleContext } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { t } from '../src/lib/type_builders';
import { Range } from '../src/server/range';
import { makeTableView } from '../src/server/runtime';

type MembershipRow = { id: bigint; tenant: string; email: string };

/**
 * A table whose unique constraint covers two columns, with a btree index on a
 * proper subset of them (`tenant`) alongside one that matches the constraint
 * exactly (`tenant, email`).
 *
 * `ConstraintOpts` currently types `columns` as a one-column tuple, so the
 * composite constraint is spelled out through the raw constraint shape. Every
 * layer underneath already handles N columns: `table()` maps over
 * `constraintOpts.columns`, `RawConstraintDefV10`'s `Unique` payload is a
 * column list, and `crates/codegen` emits multi-column unique constraints.
 */
function membershipTable() {
  const ctx = new ModuleContext();
  const membership = table(
    {
      name: 'membership',
      indexes: [
        {
          accessor: 'byTenant',
          name: 'membership_tenant_idx_btree',
          algorithm: 'btree',
          columns: ['tenant'] as const,
        },
        {
          accessor: 'byTenantEmail',
          name: 'membership_tenant_email_idx_btree',
          algorithm: 'btree',
          columns: ['tenant', 'email'] as const,
        },
      ] as const,
      constraints: [
        {
          name: 'membership_tenant_email_key',
          constraint: 'unique',
          columns: ['tenant', 'email'],
        },
      ] as unknown as ConstraintOpts<'id' | 'tenant' | 'email'>[],
    },
    {
      id: t.u64().primaryKey().autoInc(),
      tenant: t.string(),
      email: t.string(),
    }
  );

  const rawTableDef = membership.tableDef(ctx, 'membership');
  const view = makeTableView(ctx.typespace, rawTableDef) as any;
  const rowType = ctx.typespace.types[rawTableDef.productTypeRef];
  const serializeRow = AlgebraicType.makeSerializer(rowType, ctx.typespace);

  const encodeRows = (rows: MembershipRow[]): Uint8Array => {
    const writer = new BinaryWriter(1024);
    for (const row of rows) serializeRow(writer, row);
    return writer.getBuffer();
  };

  return { view, encodeRows };
}

describe('index uniqueness is decided by an exact column match', () => {
  it('leaves a btree index on a prefix of a composite unique constraint ranged', () => {
    const { view, encodeRows } = membershipTable();
    const rows: MembershipRow[] = [
      { id: 1n, tenant: 'acme', email: 'ada@acme.test' },
      { id: 2n, tenant: 'acme', email: 'grace@acme.test' },
    ];

    // `tenant` alone is not unique: two rows share it, so the index has to be
    // the ranged kind that can return all of them.
    expect(view.byTenant.filter).toBeInstanceOf(Function);
    expect(view.byTenant.find).toBeUndefined();

    host.reset(encodeRows(rows));
    expect([...view.byTenant.filter('acme')]).toEqual(rows);
    expect(host.pointScans).toBe(1);

    // A ranged index also has to accept a `Range` and take the range-scan path.
    host.reset(encodeRows(rows));
    const range = new Range<string>(
      { tag: 'included', value: 'acme' },
      { tag: 'excluded', value: 'zenith' }
    );
    expect([...view.byTenant.filter(range)]).toEqual(rows);
    expect(host.rangeScans).toBe(1);
  });

  it('keeps the index matching the composite unique constraint unique', () => {
    const { view, encodeRows } = membershipTable();
    const row: MembershipRow = {
      id: 1n,
      tenant: 'acme',
      email: 'ada@acme.test',
    };

    expect(view.byTenantEmail.find).toBeInstanceOf(Function);
    expect(view.byTenantEmail.filter).toBeUndefined();

    host.reset(encodeRows([row]));
    expect(view.byTenantEmail.find(['acme', 'ada@acme.test'])).toEqual(row);
    expect(host.pointScans).toBe(1);
  });

  it('keeps the primary-key index unique and updatable', () => {
    const { view, encodeRows } = membershipTable();
    const row: MembershipRow = {
      id: 1n,
      tenant: 'acme',
      email: 'ada@acme.test',
    };

    expect(view.id.find).toBeInstanceOf(Function);
    expect(view.id.update).toBeInstanceOf(Function);
    expect(view.id.filter).toBeUndefined();

    host.reset(encodeRows([row]));
    expect(view.id.find(1n)).toEqual(row);
    expect(host.pointScans).toBe(1);
  });
});
