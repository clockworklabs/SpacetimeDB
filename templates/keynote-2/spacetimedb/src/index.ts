import { schema, table, t, SenderError } from 'spacetimedb/server';

const spacetimedb = schema({
  accounts: table(
    { name: 'accounts', public: true },
    {
      id: t.u32().primaryKey().index('hash'),
      balance: t.i64(),
    },
  ),
  transfer_audit: table(
    { name: 'transfer_audit', public: true },
    {
      id: t.u64().primaryKey().autoInc(),
      from: t.u32(),
      to: t.u32(),
      amount: t.i64(),
      ts: t.timestamp(),
    },
  ),
});
export default spacetimedb;

export const seed = spacetimedb.reducer(
  { n: t.u32(), initialBalance: t.i64() },
  (ctx, { n, initialBalance: balance }) => {
    const accounts = ctx.db.accounts;

    for (const row of accounts) {
      accounts.delete(row);
    }

    for (let id = 0; id < n; id++) {
      accounts.insert({ id, balance });
    }
  },
);

export const transfer = spacetimedb.reducer(
  { from: t.u32(), to: t.u32(), amount: t.i64() },
  (ctx, { from, to, amount }) => {
    if (from === to) {
      throw new SenderError('same_account');
    }
    if (amount <= 0) {
      throw new SenderError('non_positive_amount');
    }

    const accounts = ctx.db.accounts;
    const byId = accounts.id;

    const fromRow = byId.find(from);
    const toRow = byId.find(to);
    if (fromRow === null || toRow === null) {
      throw new SenderError('account_missing');
    }

    if (fromRow.balance < amount) {
      throw new SenderError('insufficient_funds');
    }

    byId.update({
      id: from,
      balance: fromRow.balance - amount,
    });

    byId.update({
      id: to,
      balance: toRow.balance + amount,
    });
  },
);

// Multi-step transfer + fraud check + audit, all inside one reducer.
// Mirrors the "typical app flow": (1) read source balance, (2) check
// against fraud limit, (3) apply transfer, (4) write an audit row.
export const transfer_with_audit = spacetimedb.reducer(
  {
    from: t.u32(),
    to: t.u32(),
    amount: t.i64(),
    fraudLimit: t.i64(),
  },
  (ctx, { from, to, amount, fraudLimit }) => {
    if (from === to) throw new SenderError('same_account');
    if (amount <= 0) throw new SenderError('non_positive_amount');
    if (amount > fraudLimit) throw new SenderError('fraud_limit_exceeded');

    const accounts = ctx.db.accounts;
    const byId = accounts.id;

    const fromRow = byId.find(from);
    const toRow = byId.find(to);
    if (fromRow === null || toRow === null) {
      throw new SenderError('account_missing');
    }
    if (fromRow.balance < amount) {
      throw new SenderError('insufficient_funds');
    }

    byId.update({ id: from, balance: fromRow.balance - amount });
    byId.update({ id: to, balance: toRow.balance + amount });

    ctx.db.transfer_audit.insert({
      id: 0n,
      from,
      to,
      amount,
      ts: ctx.timestamp,
    });
  },
);
