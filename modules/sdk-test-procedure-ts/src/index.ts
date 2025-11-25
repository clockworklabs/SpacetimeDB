// ─────────────────────────────────────────────────────────────────────────────
// IMPORTS
// ─────────────────────────────────────────────────────────────────────────────
import { errors, schema, t, table } from 'spacetimedb/server';

const ReturnStruct = t.object('ReturnStruct', {
  a: t.u32(),
  b: t.string(),
});

const ReturnEnum = t.enum('ReturnEnum', {
  A: t.u32(),
  B: t.string(),
});

const spacetimedb = schema();

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
