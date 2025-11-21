// ─────────────────────────────────────────────────────────────────────────────
// IMPORTS
// ─────────────────────────────────────────────────────────────────────────────
import { toCamelCase, type Infer } from 'spacetimedb';
import {
  errors,
  type ProcedureCtx,
  type RowObj,
  schema,
  t,
  table,
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

const spacetimedb = schema(MyTable);

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

spacetimedb.procedure('will_panic', t.unit(), ctx => {
  throw new Error('This procedure is expected to panic');
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
  ctx.with_tx(ctx => {
    assertEqual(ctx.db.myTable.count(), BigInt(count));
  });
}

function assertEqual<T>(a: T, b: T) {
  if (a !== b) {
    throw new Error(`assertion failed: ${a} != ${b}`);
  }
}

spacetimedb.procedure('insert_with_tx_commit', t.unit(), ctx => {
  ctx.with_tx(insertMyTable);
  assertRowCount(ctx, 1);
  return {};
});

spacetimedb.procedure('insert_with_tx_rollback', t.unit(), ctx => {
  const error = {};
  try {
    ctx.with_tx(ctx => {
      insertMyTable(ctx);
      throw error;
    });
  } catch (e) {
    if (e !== error) throw e;
  }
  assertRowCount(ctx, 0);
  return {};
});
