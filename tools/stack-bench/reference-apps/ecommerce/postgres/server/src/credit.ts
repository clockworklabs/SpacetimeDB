import type { Express, RequestHandler } from 'express';
import type { Pool, PoolClient } from 'pg';

export async function initializeCredit(pool: Pool) {
  await pool.query(`
    ALTER TABLE account ADD COLUMN IF NOT EXISTS credit_minor bigint NOT NULL DEFAULT 0 CHECK (credit_minor >= 0);
    ALTER TABLE orders ADD COLUMN IF NOT EXISTS credit_minor bigint NOT NULL DEFAULT 0;
    ALTER TABLE orders ADD COLUMN IF NOT EXISTS external_minor bigint NOT NULL DEFAULT 0;
    CREATE TABLE IF NOT EXISTS credit_entry(id bigserial PRIMARY KEY, account_id integer NOT NULL REFERENCES account(id),
      reference text NOT NULL, amount_minor bigint NOT NULL, UNIQUE(account_id,reference));
  `);
}

export async function spendCredit(client: PoolClient, accountId: number, totalMinor: number, orderId: number) {
  const account = await client.query('SELECT credit_minor FROM account WHERE id=$1 FOR UPDATE', [accountId]);
  const creditMinor = Math.min(Number(account.rows[0].credit_minor), totalMinor);
  if (creditMinor) {
    await client.query('UPDATE account SET credit_minor=credit_minor-$1 WHERE id=$2', [creditMinor, accountId]);
    await client.query('INSERT INTO credit_entry(account_id,reference,amount_minor) VALUES($1,$2,$3)', [accountId, `order:${orderId}`, -creditMinor]);
  }
  return creditMinor;
}

export function refundForReturn(total: number, refundedTotal: number, gross: number, returnedGross: number, allReturned: boolean): number {
  const remaining = Math.max(0, Math.round((total - refundedTotal) * 100) / 100);
  return allReturned ? remaining : gross > 0
    ? Math.min(remaining, Math.round(returnedGross * total / gross * 100) / 100) : 0;
}

export async function refundCredit(client: PoolClient, order: any, refundedTotal = Number(order.total)) {
  if (Number(order.total) <= 0) return;
  const amount = Math.min(Number(order.credit_minor), Math.round(Number(order.credit_minor) * refundedTotal / Number(order.total)));
  if (!amount) return;
  const previous = await client.query('SELECT amount_minor FROM credit_entry WHERE account_id=$1 AND reference=$2', [order.account_id, `refund:${order.id}`]);
  const delta = amount - Number(previous.rows[0]?.amount_minor ?? 0);
  if (delta <= 0) return;
  await client.query('INSERT INTO credit_entry(account_id,reference,amount_minor) VALUES($1,$2,$3) ON CONFLICT(account_id,reference) DO UPDATE SET amount_minor=excluded.amount_minor',
    [order.account_id, `refund:${order.id}`, amount]);
  await client.query('UPDATE account SET credit_minor=credit_minor+$1 WHERE id=$2', [delta, order.account_id]);
}

export function registerCredit(app: Express, pool: Pool, auth: RequestHandler, staff: RequestHandler,
  checkout: (accountId: number, useCredit: boolean) => Promise<number>, changed: (accountId: number) => Promise<void>) {
  app.get('/api/credit', auth, async (req, res, next) => {
    try {
      const accountId = (req as any).account.id;
      const account = (await pool.query('SELECT * FROM account WHERE id=$1', [accountId])).rows[0];
      const entries = await pool.query('SELECT id,reference,amount_minor FROM credit_entry WHERE account_id=$1 ORDER BY id', [accountId]);
      const customers = account.is_admin || account.is_staff ? await pool.query('SELECT id,username AS name FROM account WHERE NOT is_admin AND NOT is_staff ORDER BY id') : { rows: [] };
      res.json({ accountId: String(accountId), balance: Number(account.credit_minor) / 100,
        entries: entries.rows.map(row => ({ id: String(row.id), reference: row.reference, amount: Number(row.amount_minor) / 100 })), customers: customers.rows });
    } catch (error) { next(error); }
  });
  app.post('/api/staff/credit', auth, staff, async (req, res) => {
    const accountId = Number(req.body.accountId), amountMinor = req.body.amountMinor, reference = req.body.reference;
    if (!Number.isSafeInteger(accountId) || accountId < 1 || !Number.isSafeInteger(amountMinor) || amountMinor < 1 || typeof reference !== 'string' || !reference.trim()) return res.status(400).json({ error: 'Invalid credit grant' });
    const client = await pool.connect();
    try {
      await client.query('BEGIN');
      const target = await client.query('SELECT credit_minor FROM account WHERE id=$1 FOR UPDATE', [accountId]);
      if (!target.rows.length || !Number.isSafeInteger(Number(target.rows[0].credit_minor) + amountMinor)) throw new Error('Invalid customer balance');
      const existing = await client.query('SELECT amount_minor FROM credit_entry WHERE account_id=$1 AND reference=$2', [accountId, reference]);
      if (existing.rows.length && Number(existing.rows[0].amount_minor) !== amountMinor) throw new Error('Reference identifies another grant');
      if (!existing.rows.length) {
        await client.query('INSERT INTO credit_entry(account_id,reference,amount_minor) VALUES($1,$2,$3)', [accountId, reference, amountMinor]);
        await client.query('UPDATE account SET credit_minor=credit_minor+$1 WHERE id=$2', [amountMinor, accountId]);
      }
      await client.query('COMMIT'); res.json({ ok: true });
    } catch (error) { await client.query('ROLLBACK'); res.status(409).json({ error: String(error) }); }
    finally { client.release(); }
  });
  app.post('/api/checkout/credit', auth, async (req, res) => {
    const accountId = (req as any).account.id;
    try { const orderId = await checkout(accountId, true); await changed(accountId); res.json({ orderId }); }
    catch (error) { res.status(409).json({ error: String(error) }); }
  });
}
