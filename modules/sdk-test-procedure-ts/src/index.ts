// ─────────────────────────────────────────────────────────────────────────────
// IMPORTS
// ─────────────────────────────────────────────────────────────────────────────
import { ScheduleAt } from 'spacetimedb';
import {
  errors,
  schema,
  t,
  table,
  type ProcedureCtx,
  type TransactionCtx,
} from 'spacetimedb/server';

const ReturnStruct = t.object('ReturnStruct', {
  a: t.u32(),
  b: t.string(),
});

const ReturnEnum = t.enum('ReturnEnum', {
  A: t.u32(),
  B: t.string(),
});

const MyTable = table(
  { name: 'my_table', public: true },
  { field: ReturnStruct }
);

const ScheduledProcTable = t.row({
  scheduled_id: t.u64().primaryKey().autoInc(),
  scheduled_at: t.scheduleAt(),
  reducer_ts: t.timestamp(),
  x: t.u8(),
  y: t.u8(),
});
const ScheduledProcTableTable = table(
  { name: 'scheduled_proc_table', scheduled: 'scheduled_proc' },
  ScheduledProcTable
);

const ProcInsertsInto = t.row({
  reducer_ts: t.timestamp(),
  procedure_ts: t.timestamp(),
  x: t.u8(),
  y: t.u8(),
});
const ProcInsertsIntoTable = table(
  { name: 'proc_inserts_into', public: true },
  ProcInsertsInto
);

const spacetimedb = schema(MyTable, ScheduledProcTableTable, ProcInsertsIntoTable);

spacetimedb.procedure(
  'return_primitive',
  { lhs: t.u32(), rhs: t.u32() },
  t.u32(),
  (_ctx, { lhs, rhs }) => lhs + rhs
);

spacetimedb.procedure(
  'return_struct',
  { a: t.u32(), b: t.string() },
  ReturnStruct,
  (_ctx, { a, b }) => ({ a, b })
);

spacetimedb.procedure(
  'return_enum_a',
  { a: t.u32() },
  ReturnEnum,
  (_ctx, { a }) => ReturnEnum.A(a)
);

spacetimedb.procedure(
  'return_enum_b',
  { b: t.string() },
  ReturnEnum,
  (_ctx, { b }) => ReturnEnum.B(b)
);

spacetimedb.procedure('will_panic', t.unit(), _ctx => {
  throw new Error('This procedure is expected to panic');
});

spacetimedb.procedure('read_my_schema', t.string(), ctx => {
  const module_identity = ctx.identity;
  const response = ctx.http.fetch(
    `http://localhost:3000/v1/database/${module_identity}/schema?version=9`
  );
  return response.text();
});

spacetimedb.procedure('invalid_request', t.string(), ctx => {
  try {
    const response = ctx.http.fetch('http://foo.invalid/');
    throw new Error(
      `Got result from requesting \`http://foo.invalid\`... huh?\n${response.text()}`
    );
  } catch (e) {
    if (e instanceof errors.HttpError) {
      return e.message;
    }
    throw e;
  }
});

function insertMyTable(ctx: TransactionCtx<typeof spacetimedb.schemaType>) {
  ctx.db.myTable.insert({ field: { a: 42, b: 'magic' } });
}

function assertRowCount(
  ctx: ProcedureCtx<typeof spacetimedb.schemaType>,
  count: number
) {
  ctx.withTx(ctx => {
    assertEqual(ctx.db.myTable.count(), BigInt(count));
  });
}

function assertEqual<T>(a: T, b: T) {
  if (a !== b) {
    throw new Error(`assertion failed: ${a} != ${b}`);
  }
}

spacetimedb.procedure('insert_with_tx_commit', t.unit(), ctx => {
  ctx.withTx(insertMyTable);
  assertRowCount(ctx, 1);
  return {};
});

spacetimedb.procedure('insert_with_tx_rollback', t.unit(), ctx => {
  const error = {};
  try {
    ctx.withTx(ctx => {
      insertMyTable(ctx);
      throw error;
    });
  } catch (e) {
    if (e !== error) throw e;
  }
  assertRowCount(ctx, 0);
  return {};
});

spacetimedb.reducer('schedule_proc', {}, ctx => {
  ctx.db.scheduledProcTable.insert({
    scheduled_id: 0n,
    scheduled_at: ScheduleAt.interval(1000000n),
    reducer_ts: ctx.timestamp,
    x: 42,
    y: 24,
  })
});

spacetimedb.procedure('scheduled_proc', { data: ScheduledProcTable }, t.unit(), (ctx, { data }) => {
  const reducer_ts = data.reducer_ts;
  const x = data.x;
  const y = data.y;
  const procedure_ts = ctx.timestamp;
  ctx.withTx(ctx => {
    ctx.db.procInsertsInto.insert({
      reducer_ts,
      procedure_ts,
      x,
      y
    });
  });
  return {};
});
