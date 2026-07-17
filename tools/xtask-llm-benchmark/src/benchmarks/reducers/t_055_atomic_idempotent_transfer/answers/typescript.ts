import { schema, table, t } from 'spacetimedb/server';

const account = table({ name: 'account', public: true }, { id: t.u64().primaryKey(), balance: t.i64() });
const transferRequest = table(
  { name: 'transfer_request', public: true },
  { requestId: t.string().primaryKey(), fromId: t.u64(), toId: t.u64(), amount: t.i64() }
);
const spacetimedb = schema({ account, transferRequest });
export default spacetimedb;

export const create_account = spacetimedb.reducer(
  { id: t.u64(), balance: t.i64() },
  (ctx, { id, balance }) => ctx.db.account.insert({ id, balance })
);

export const transfer = spacetimedb.reducer(
  { requestId: t.string(), fromId: t.u64(), toId: t.u64(), amount: t.i64() },
  (ctx, { requestId, fromId, toId, amount }) => {
    if (ctx.db.transferRequest.requestId.find(requestId)) return;
    if (amount <= 0n || fromId === toId) throw new Error('invalid transfer');
    const from = ctx.db.account.id.find(fromId);
    const to = ctx.db.account.id.find(toId);
    if (!from || !to) throw new Error('account not found');
    if (from.balance < amount) throw new Error('insufficient balance');
    ctx.db.account.id.update({ ...from, balance: from.balance - amount });
    ctx.db.account.id.update({ ...to, balance: to.balance + amount });
    ctx.db.transferRequest.insert({ requestId, fromId, toId, amount });
  }
);
