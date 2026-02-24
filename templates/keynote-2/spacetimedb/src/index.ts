import { schema, table, t, SenderError } from 'spacetimedb/server';

const spacetimedb = schema({
  account: table(
    { name: 'account' },
    {
      id: t.u32().primaryKey().index('hash'),
      balance: t.i64(),
    },
  ),
});
export default spacetimedb;

export const seed = spacetimedb.reducer(
  { n: t.u32(), balance: t.i64() },
  (ctx, { n, balance }) => {
    const accounts = ctx.db.account;

    for (const row of accounts) {
      accounts.delete(row);
    }

    for (let id = 0; id < n; id++) {
      accounts.insert({ id, balance });
    }
  },
);

export const transfer = spacetimedb.reducer(
  { from: t.u32(), to: t.u32(), amount: t.u32() },
  (ctx, { from, to, amount: amt }) => {
    const accounts = ctx.db.account;
    const byId = accounts.id;

    const fromRow = byId.find(from)!;
    const toRow = byId.find(to)!;

    const amount = BigInt(amt);
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
