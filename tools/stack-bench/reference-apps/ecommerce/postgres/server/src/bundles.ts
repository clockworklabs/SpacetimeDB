import { refundCredit, refundForReturn } from './credit.js';
import type { Express, RequestHandler } from 'express';
import type { Pool, PoolClient } from 'pg';

type Allocation = { itemId: number; warehouseId: number; quantity: number };
export async function releaseBundle(client: PoolClient, allocations: Allocation[]) {
  for (const row of allocations) await client.query(
    'UPDATE stock SET quantity = quantity + $1 WHERE item_id = $2 AND warehouse_id = $3',
    [row.quantity, row.itemId, row.warehouseId]);
}

export async function reserveBundle(client: PoolClient, components: readonly { itemId: number; quantity: number }[]): Promise<Allocation[]> {
  const allocations: Allocation[] = [];
  for (const component of [...components].sort((a, b) => a.itemId - b.itemId)) {
    const rows = await client.query('SELECT warehouse_id,quantity FROM stock WHERE item_id=$1 ORDER BY warehouse_id FOR UPDATE', [component.itemId]);
    if (rows.rows.reduce((sum, row) => sum + row.quantity, 0) < component.quantity) throw new Error('A component is unavailable');
    let remaining = component.quantity;
    for (const row of rows.rows) {
      const quantity = Math.min(remaining, row.quantity);
      if (!quantity) continue;
      await client.query('UPDATE stock SET quantity=quantity-$1 WHERE item_id=$2 AND warehouse_id=$3', [quantity, component.itemId, row.warehouse_id]);
      allocations.push({ itemId: component.itemId, warehouseId: row.warehouse_id, quantity });
      remaining -= quantity;
    }
  }
  return allocations;
}

export async function initializeBundles(pool: Pool) {
  await pool.query(`
    ALTER TABLE item ADD COLUMN IF NOT EXISTS bundle_components jsonb NOT NULL DEFAULT '[]';
    CREATE UNIQUE INDEX IF NOT EXISTS item_name_unique ON item(name);
    ALTER TABLE cart_item ADD COLUMN IF NOT EXISTS bundle_components_json text NOT NULL DEFAULT '';
    ALTER TABLE cart_item ADD COLUMN IF NOT EXISTS bundle_price numeric(12,2);
    ALTER TABLE cart_item ADD COLUMN IF NOT EXISTS component_allocations jsonb NOT NULL DEFAULT '[]';
    ALTER TABLE order_item ADD COLUMN IF NOT EXISTS is_bundle boolean NOT NULL DEFAULT false;
    ALTER TABLE order_item ADD COLUMN IF NOT EXISTS component_allocations jsonb NOT NULL DEFAULT '[]';
  `);
}

export function registerBundles(app: Express, pool: Pool, auth: RequestHandler,
  changed: (accountId: number) => Promise<void>) {
  app.get('/api/bundles', async (_req, res, next) => {
    try {
      const rows = await pool.query(`SELECT id, name, price, bundle_components AS components FROM item WHERE bundle_components <> '[]'::jsonb ORDER BY id`);
      res.json(rows.rows);
    } catch (error) { next(error); }
  });
  app.post('/api/bundles', auth, async (req, res, next) => {
    const actor = (await pool.query('SELECT is_admin,staff_role FROM account WHERE id=$1', [(req as any).account.id])).rows[0];
    if (!actor?.is_admin && actor?.staff_role !== 'catalog') return res.status(403).json({error:'Catalog access required'});
    const name = typeof req.body.name === 'string' ? req.body.name.trim() : '';
    const price = Number(req.body.price);
    let components: Array<{ item: string; quantity: number }>;
    try { components = JSON.parse(req.body.componentsJson); } catch { return res.status(400).json({ error: 'Invalid components' }); }
    if (!name || !Number.isFinite(price) || price <= 0 || !Array.isArray(components) || !components.length
      || components.some(value => !value || typeof value.item !== 'string' || !Number.isSafeInteger(value.quantity) || value.quantity < 1)
      || new Set(components.map(value => value.item)).size !== components.length) return res.status(400).json({ error: 'Invalid bundle' });
    const client = await pool.connect();
    try {
      await client.query('BEGIN');
      const values = [];
      for (const component of components) {
        const item = await client.query(`SELECT id FROM item WHERE name = $1 AND bundle_components = '[]'::jsonb`, [component.item]);
        if (item.rows.length !== 1) throw new Error('Unknown product');
        values.push({ ...component, itemId: item.rows[0].id });
      }
      const existing = await client.query('SELECT id, bundle_components FROM item WHERE name = $1 FOR UPDATE', [name]);
      if (existing.rows.length && !existing.rows[0].bundle_components.length) throw new Error('A product already uses this name');
      const row = existing.rows.length
        ? await client.query('UPDATE item SET price = $1, bundle_components = $2 WHERE id = $3 RETURNING id', [price, JSON.stringify(values), existing.rows[0].id])
        : await client.query("INSERT INTO item(name,price,category,bundle_components) VALUES ($1,$2,'Bundles',$3) RETURNING id", [name, price, JSON.stringify(values)]);
      await client.query('COMMIT');
      res.json(row.rows[0]);
    } catch (error) { await client.query('ROLLBACK'); res.status(400).json({ error: String(error) }); }
    finally { client.release(); }
  });
  app.post('/api/cart/bundles', auth, async (req, res) => {
    const accountId = (req as any).account.id;
    const bundleId = Number(req.body.bundleId);
    if (!Number.isSafeInteger(bundleId) || bundleId < 1) return res.status(400).json({ error: 'Invalid bundle' });
    const client = await pool.connect();
    try {
      await client.query('BEGIN');
      const bundles = await client.query('SELECT id, price, bundle_components FROM item WHERE id=$1', [bundleId]);
      const bundle = bundles.rows[0];
      if (!bundle?.bundle_components.length) throw new Error('Bundle not found');
      await client.query('INSERT INTO cart(account_id) VALUES ($1) ON CONFLICT(account_id) DO NOTHING', [accountId]);
      const cart = await client.query('SELECT id FROM cart WHERE account_id=$1 FOR UPDATE', [accountId]);
      const current = await client.query('SELECT id, component_allocations, reserved_until FROM cart_item WHERE cart_id=$1 AND item_id=$2', [cart.rows[0].id, bundleId]);
      if (current.rows[0] && new Date(current.rows[0].reserved_until) > new Date()) throw new Error('Bundle already in cart');
      if (current.rows[0]) {
        await releaseBundle(client, current.rows[0].component_allocations);
        await client.query('DELETE FROM cart_item WHERE id=$1', [current.rows[0].id]);
      }
      const allocations = await reserveBundle(client, bundle.bundle_components);
      await client.query(`INSERT INTO cart_item(cart_id,item_id,quantity,bundle_price,component_allocations,reserved_until,expired,bundle_components_json)
        VALUES($1,$2,1,$3,$4,now()+interval '90 seconds',false,$5)`, [cart.rows[0].id, bundleId, bundle.price, JSON.stringify(allocations), JSON.stringify(bundle.bundle_components)]);
      await client.query('UPDATE cart SET last_activity=now(),expired_at=null WHERE id=$1', [cart.rows[0].id]);
      await client.query('COMMIT'); await changed(accountId); res.json({ ok: true });
    } catch (error) { await client.query('ROLLBACK'); res.status(409).json({ error: String(error) }); }
    finally { client.release(); }
  });
  app.post('/api/bundle-orders/:orderId/return', auth, async (req, res) => {
    const accountId = (req as any).account.id;
    const orderId = Number(req.params.orderId);
    if (!Number.isSafeInteger(orderId) || orderId < 1) return res.status(400).json({ error: 'Invalid order' });
    const client = await pool.connect();
    try {
      await client.query('BEGIN');
      const order = await client.query("SELECT * FROM orders WHERE id=$1 AND account_id=$2 AND status IN ('shipped','delivered') FOR UPDATE", [orderId, accountId]);
      if (!order.rows.length) throw new Error('No returnable bundle on this account');
      const lines = await client.query('SELECT * FROM order_item WHERE order_id=$1 AND is_bundle AND NOT returned FOR UPDATE', [orderId]);
      if (!lines.rows.length) throw new Error('Bundle already returned');
      let refund = 0;
      for (const line of lines.rows) {
        await releaseBundle(client, line.component_allocations);
        await client.query('UPDATE order_item SET returned=true WHERE id=$1', [line.id]);
        refund += Number(line.price) * line.quantity;
      }
      const gross = await client.query('SELECT SUM(price * quantity) AS total, BOOL_AND(returned) AS all_returned FROM order_item WHERE order_id=$1', [orderId]);
      refund = refundForReturn(Number(order.rows[0].total), Number(order.rows[0].refund_total), Number(gross.rows[0].total), refund, gross.rows[0].all_returned);
      await refundCredit(client, order.rows[0], Number(order.rows[0].refund_total) + refund);
      await client.query('UPDATE orders SET refund_total=refund_total+$1 WHERE id=$2', [refund, orderId]);
      await client.query('COMMIT'); await changed(accountId); res.json({ ok: true });
    } catch (error) { await client.query('ROLLBACK'); res.status(403).json({ error: String(error) }); }
    finally { client.release(); }
  });
}
