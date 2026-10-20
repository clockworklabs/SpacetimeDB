import type { Pool } from 'pg';

export async function initializeOrderData(pool: Pool) {
  await pool.query(`
    CREATE OR REPLACE VIEW order_account AS SELECT id, username FROM account;
    CREATE OR REPLACE VIEW order_header AS
      SELECT id, account_id, total,
        CASE WHEN status = 'cancelled' THEN total ELSE refund_total END AS refunded, status FROM orders;
    CREATE OR REPLACE VIEW order_line AS
      SELECT id, order_id, item_id, quantity, price AS unit_price FROM order_item;
    CREATE OR REPLACE VIEW order_cart AS
      SELECT c.account_id, ci.item_id, ci.quantity FROM cart_item ci JOIN cart c ON c.id = ci.cart_id;
    CREATE OR REPLACE VIEW order_reservation AS
      SELECT c.account_id, ci.item_id, r.warehouse_id, r.quantity
      FROM cart_reservation_allocation r JOIN cart_item ci ON ci.id = r.cart_item_id JOIN cart c ON c.id = ci.cart_id;
    CREATE OR REPLACE VIEW order_allocation AS
      SELECT id AS order_line_id, warehouse_id, quantity FROM order_item;
  `);
}
