import type { Express, RequestHandler } from 'express';
import type { Pool } from 'pg';

export async function initializeSubscriptions(pool: Pool) {
  await pool.query(`
    CREATE TABLE IF NOT EXISTS purchase_subscription (
      id serial PRIMARY KEY, account_id integer NOT NULL REFERENCES account(id),
      item_id integer NOT NULL REFERENCES item(id), quantity integer NOT NULL CHECK(quantity > 0),
      price numeric(10,2) NOT NULL, interval_seconds integer NOT NULL CHECK(interval_seconds >= 30),
      slots integer NOT NULL CHECK(slots BETWEEN 1 AND 12), processed integer NOT NULL DEFAULT 0,
      due_at timestamptz NOT NULL, status text NOT NULL DEFAULT 'active', paused_at timestamptz
    );
    CREATE TABLE IF NOT EXISTS subscription_delivery (
      subscription_id integer NOT NULL REFERENCES purchase_subscription(id), slot integer NOT NULL,
      status text NOT NULL, order_id integer REFERENCES orders(id), PRIMARY KEY(subscription_id, slot)
    );
  `);
}

export function registerSubscriptions(app: Express, pool: Pool, auth: RequestHandler) {
  app.get('/api/subscriptions', auth, async (req, res, next) => {
    try {
      const rows = await pool.query(`SELECT s.id, i.name AS item, s.status,
        COALESCE((SELECT sum(o.payment_amount) FROM subscription_delivery d JOIN orders o ON o.id=d.order_id
          WHERE d.subscription_id=s.id),0) AS total,
        COALESCE((SELECT json_agg(json_build_object('status',d.status) ORDER BY d.slot)
          FROM subscription_delivery d WHERE d.subscription_id=s.id),'[]') AS deliveries
        FROM purchase_subscription s JOIN item i ON i.id=s.item_id WHERE s.account_id=$1 ORDER BY s.id`, [req.account!.id]);
      res.json(rows.rows.map(row => ({ ...row, total: Number(row.total) })));
    } catch (error) { next(error); }
  });
  app.post('/api/subscriptions', auth, async (req, res, next) => {
    const { item, quantity, intervalSeconds, deliveries } = req.body;
    if (typeof item !== 'string' || !Number.isSafeInteger(quantity) || quantity < 1 || quantity > 1000
      || !Number.isSafeInteger(intervalSeconds) || intervalSeconds < 30 || intervalSeconds > 31536000
      || !Number.isSafeInteger(deliveries) || deliveries < 1 || deliveries > 12) {
      return res.status(400).json({ error: 'Invalid subscription' });
    }
    try {
      const created = await pool.query(`INSERT INTO purchase_subscription
        (account_id,item_id,quantity,price,interval_seconds,slots,due_at)
        SELECT $1,id,$3,price,$4::integer,$5,now()+$4::integer*interval '1 second'
        FROM item WHERE name=$2 AND bundle_components='[]'::jsonb RETURNING id`,
      [req.account!.id, item, quantity, intervalSeconds, deliveries]);
      if (!created.rows.length) return res.status(400).json({ error: 'Choose an individual catalog item' });
      res.json(created.rows[0]);
    } catch (error) { next(error); }
  });
  for (const action of ['pause', 'resume', 'cancel'] as const) {
    app.post(`/api/subscriptions/:id/${action}`, auth, async (req, res, next) => {
      const id = Number(req.params.id);
      if (!Number.isSafeInteger(id) || id < 1) return res.status(400).json({ error: 'Invalid subscription' });
      const client = await pool.connect();
      try {
        await client.query('BEGIN');
        const result = await client.query('SELECT * FROM purchase_subscription WHERE id=$1 FOR UPDATE', [id]);
        const row = result.rows[0];
        if (!row || row.account_id !== req.account!.id) {
          await client.query('ROLLBACK');
          return res.status(403).json({ error: 'Subscription access denied' });
        }
        if (action === 'pause' && row.status === 'active') {
          await client.query("UPDATE purchase_subscription SET status='paused',paused_at=now() WHERE id=$1", [id]);
        } else if (action === 'resume' && row.status === 'paused') {
          await client.query("UPDATE purchase_subscription SET status='active',due_at=due_at+(now()-paused_at),paused_at=null WHERE id=$1", [id]);
        } else if (action === 'cancel' && ['active','paused'].includes(row.status)) {
          await client.query("UPDATE purchase_subscription SET status='cancelled',paused_at=null WHERE id=$1", [id]);
        }
        await client.query('COMMIT');
        res.json({ ok: true });
      } catch (error) { await client.query('ROLLBACK'); next(error); }
      finally { client.release(); }
    });
  }
}

export async function processSubscriptions(pool: Pool, changed: (accountId: number) => Promise<void>) {
  // A term has at most twelve slots. Later ticks continue any remaining backlog.
  for (let count = 0; count < 12; count += 1) {
    const client = await pool.connect();
    let accountId: number;
    try {
      await client.query('BEGIN');
      const next = await client.query(`SELECT s.*,i.name FROM purchase_subscription s JOIN item i ON i.id=s.item_id
        WHERE s.status='active' AND s.due_at<=now() ORDER BY s.due_at,s.id LIMIT 1 FOR UPDATE OF s SKIP LOCKED`);
      const subscription = next.rows[0];
      if (!subscription) { await client.query('COMMIT'); return; }
      accountId = subscription.account_id;
      const stock = await client.query('SELECT * FROM stock WHERE item_id=$1 ORDER BY warehouse_id FOR UPDATE', [subscription.item_id]);
      let orderId: number | null = null;
      if (stock.rows.reduce((sum, row) => sum + row.quantity, 0) >= subscription.quantity) {
        const order = await client.query(`INSERT INTO orders(account_id,total,status,payment_status,payment_amount,credit_minor,external_minor)
          VALUES($1,$2::numeric*$3::integer,'pending','paid',$2::numeric*$3::integer,0,
          round($2::numeric*$3::integer*100)) RETURNING id`, [accountId, subscription.price, subscription.quantity]);
        orderId = order.rows[0].id;
        let remaining = subscription.quantity;
        for (const row of stock.rows) {
          const quantity = Math.min(remaining, row.quantity);
          if (!quantity) continue;
          await client.query('UPDATE stock SET quantity=quantity-$1 WHERE item_id=$2 AND warehouse_id=$3',
            [quantity, subscription.item_id, row.warehouse_id]);
          await client.query(`INSERT INTO order_item(order_id,item_id,item_name,quantity,price,warehouse_id)
            VALUES($1,$2,$3,$4,$5,$6)`, [orderId, subscription.item_id, subscription.name, quantity, subscription.price, row.warehouse_id]);
          remaining -= quantity;
          if (!remaining) break;
        }
      }
      await client.query('INSERT INTO subscription_delivery(subscription_id,slot,status,order_id) VALUES($1,$2,$3,$4)',
        [subscription.id, subscription.processed + 1, orderId === null ? 'skipped' : 'paid', orderId]);
      await client.query(`UPDATE purchase_subscription SET processed=processed+1,
        status=CASE WHEN processed+1=slots THEN 'complete' ELSE 'active' END,
        due_at=due_at+interval_seconds*interval '1 second' WHERE id=$1`, [subscription.id]);
      await client.query('COMMIT');
    } catch (error) { await client.query('ROLLBACK'); throw error; }
    finally { client.release(); }
    await changed(accountId);
  }
}
