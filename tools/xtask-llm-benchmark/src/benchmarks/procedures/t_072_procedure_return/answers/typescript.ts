import { schema, t } from 'spacetimedb/server';

const Summary = t.object('Summary', { total: t.u32(), label: t.string() });
const spacetimedb = schema({});
export default spacetimedb;

export const calculate_summary = spacetimedb.procedure(
  { lhs: t.u32(), rhs: t.u32() }, Summary,
  (_ctx, { lhs, rhs }) => ({ total: lhs + rhs, label: 'calculated' })
);
