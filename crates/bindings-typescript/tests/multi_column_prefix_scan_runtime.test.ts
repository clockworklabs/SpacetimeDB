import { beforeEach, describe, expect, it, vi } from 'vitest';

const sys2_0 = vi.hoisted(() => ({
  moduleHooks: Symbol('moduleHooks'),
  register_hooks: vi.fn(),
  table_id_from_name: vi.fn(() => 1),
  index_id_from_name: vi.fn(() => 2),
  datastore_table_row_count: vi.fn(() => 0n),
  datastore_table_scan_bsatn: vi.fn(() => 10),
  datastore_index_scan_range_bsatn: vi.fn(() => 11),
  row_iter_bsatn_advance: vi.fn(() => 0),
  row_iter_bsatn_close: vi.fn(),
  datastore_insert_bsatn: vi.fn(() => 0),
  datastore_update_bsatn: vi.fn(() => 0),
  datastore_delete_by_index_scan_range_bsatn: vi.fn(() => 0),
  datastore_delete_all_by_eq_bsatn: vi.fn(() => 0),
  volatile_nonatomic_schedule_immediate: vi.fn(),
  console_log: vi.fn(),
  console_timer_start: vi.fn(() => 0),
  console_timer_end: vi.fn(),
  identity: vi.fn(() => 0n),
  get_jwt_payload: vi.fn(() => new Uint8Array()),
  procedure_http_request: vi.fn(() => [{}, new Uint8Array()]),
  procedure_start_mut_tx: vi.fn(() => 0n),
  procedure_commit_mut_tx: vi.fn(),
  procedure_abort_mut_tx: vi.fn(),
  datastore_index_scan_point_bsatn: vi.fn(() => 12),
  datastore_delete_by_index_scan_point_bsatn: vi.fn(() => 0),
}));

const sys2_1 = vi.hoisted(() => ({
  datastore_clear: vi.fn(() => 0n),
}));

vi.mock('spacetime:sys@2.0', () => sys2_0);
vi.mock('spacetime:sys@2.1', () => sys2_1);

async function buildHooks(reducer: (ctx: any) => void) {
  const [{ schema, table, t }, { moduleHooks }] = await Promise.all([
    import('../src/server'),
    import('spacetime:sys@2.0'),
  ]);

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

  const stdb = schema({ tally });
  const repro = stdb.reducer(reducer);
  return (stdb as any)[moduleHooks]({ default: stdb, repro });
}

function invokeReducer(hooks: {
  __call_reducer__(
    reducerId: number,
    sender: bigint,
    connId: bigint,
    timestamp: bigint,
    argsBuf: DataView
  ): void;
}) {
  hooks.__call_reducer__(0, 0n, 0n, 0n, new DataView(new ArrayBuffer(0)));
}

describe('server runtime multi-column prefix scans', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('uses a point scan for full-key multi-column lookups', async () => {
    const hooks = await buildHooks(ctx => {
      void [...ctx.db.tally.by_board_def.filter([1n, 'regolith'])].length;
    });

    expect(() => invokeReducer(hooks)).not.toThrow();
    expect(sys2_0.datastore_index_scan_point_bsatn).toHaveBeenCalledOnce();
    expect(sys2_0.datastore_index_scan_range_bsatn).not.toHaveBeenCalled();
  });

  it('allows bare-scalar prefix scans on multi-column btree indexes', async () => {
    const hooks = await buildHooks(ctx => {
      // Issue #5407: a single scalar should mean "prefix-scan the first column".
      void [...ctx.db.tally.by_board_def.filter(1n)].length;
    });

    expect(() => invokeReducer(hooks)).not.toThrow();
    expect(sys2_0.datastore_index_scan_range_bsatn).toHaveBeenCalledOnce();
    expect(sys2_0.datastore_index_scan_point_bsatn).not.toHaveBeenCalled();
  });
});
