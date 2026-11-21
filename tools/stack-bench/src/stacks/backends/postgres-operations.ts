import { execFileSync } from 'node:child_process';
import { checkoutStateSchema, verifyCheckoutSchema } from '../checkout-state.js';
import type { OrderDataStorage } from '../order-data.js';
import { provePostgresMarker, readPostgresOrderData, readPostgresStock, streams, writePostgresStock,
  type PsqlRunner } from '../postgres-sql.js';


import { assertLeasedContainer } from '../backend-reset-guard.js';
import type { LeasedDatabase } from '../backend-reset-guard.js';
import { POSTGRES_APPLICATION_IDENTITY } from '../hosted-database-identity.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';

const RESET_TIMEOUT_MS = 120_000;
const WRITE_TIMEOUT_MS = 60_000;
const sqlString = (value: unknown): string => `'${String(value).replaceAll("'", "''")}'`;

const POSTGRES_USER = POSTGRES_APPLICATION_IDENTITY.user;
const POSTGRES = { backend: 'postgres', label: 'PostgreSQL', quiet: false };

// psql as the application role, in the leased container checked first.
function psql(lease: LeasedDatabase, exec: TextCommandExecutor, timeout: number, purpose: string): PsqlRunner {
  const container = assertLeasedContainer(lease.resources.container, exec, timeout, purpose);
  return sql => exec('docker', ['exec', '-i', container,
    'psql', '-U', POSTGRES_USER, '-d', lease.resources.database, '-v', 'ON_ERROR_STOP=1', '-At'],
  { encoding: 'utf8', input: sql, stdio: 'pipe', timeout });
}

export function getPostgresCheckoutState({ account, item, app, lease, storage, exec = execFileSync }: {
  account: string; item: string; app: string; lease: LeasedDatabase; storage?: OrderDataStorage; exec?: TextCommandExecutor;
}) {
  if (storage?.kind === 'order-data') {
    return readPostgresOrderData(psql(lease, exec, WRITE_TIMEOUT_MS, 'order data read'), account, item, storage);
  }
  const schemaSha256 = verifyCheckoutSchema('postgres', app, ['server/src/schema.ts']);
  const run = psql(lease, exec, WRITE_TIMEOUT_MS, 'checkout state read');
  // One statement gives a consistent read of all effects. No app code or HTTP
  // response supplies these values. Payment is embedded in this reference's order.
  const sql = `WITH who AS (SELECT id FROM account WHERE username=${sqlString(account)}),
    product AS (SELECT id, price FROM item WHERE name=${sqlString(item)})
    SELECT json_build_object(
      'accountId', (SELECT id::text FROM who), 'itemId', (SELECT id::text FROM product),
      'priceMinor', (SELECT price*100 FROM product),
      'cart', COALESCE((SELECT json_agg(json_build_object('itemId', ci.item_id::text, 'quantity', ci.quantity))
        FROM cart_item ci JOIN cart c ON c.id=ci.cart_id WHERE c.account_id=(SELECT id FROM who)), '[]'::json),
      'stock', COALESCE((SELECT json_agg(json_build_object('warehouseId', warehouse_id::text, 'quantity', quantity))
        FROM stock WHERE item_id=(SELECT id FROM product)), '[]'::json),
      'reservations', COALESCE((SELECT json_agg(json_build_object('itemId', ci.item_id::text,
        'warehouseId', r.warehouse_id::text, 'quantity', r.quantity)) FROM cart_reservation_allocation r
        JOIN cart_item ci ON ci.id=r.cart_item_id JOIN cart c ON c.id=ci.cart_id WHERE c.account_id=(SELECT id FROM who)), '[]'::json),
      'orders', COALESCE((SELECT json_agg(json_build_object('id', o.id::text, 'accountId', o.account_id::text,
        'totalMinor', o.total*100, 'status', o.status, 'lines', COALESCE((SELECT json_agg(json_build_object(
          'itemId', li.item_id::text, 'quantity', li.quantity, 'priceMinor', li.price*100,
          'allocations', json_build_array(json_build_object('warehouseId', li.warehouse_id::text, 'quantity', li.quantity))))
          FROM order_item li WHERE li.order_id=o.id), '[]'::json))) FROM orders o), '[]'::json),
      'payments', COALESCE((SELECT json_agg(json_build_object('id', id::text, 'orderId', id::text,
        'amountMinor', payment_amount*100, 'status', payment_status)) FROM orders WHERE payment_status IS NOT NULL), '[]'::json),
      'orphanOrderLines', (SELECT count(*) FROM order_item li LEFT JOIN orders o ON o.id=li.order_id WHERE o.id IS NULL)
    )::text;`;
  return { schemaSha256, state: checkoutStateSchema.parse(JSON.parse(run(sql).trim())) };
}

export function getPostgresStock({ item, warehouse, lease, exec = execFileSync }: {
  item: string; warehouse?: string; lease: LeasedDatabase; exec?: TextCommandExecutor;
}): { backend: string; item: string; warehouse?: string; quantity: number } {
  return readPostgresStock(psql(lease, exec, WRITE_TIMEOUT_MS, 'direct database read'), POSTGRES, item, warehouse);
}

export function resetPostgres({ lease, exec = execFileSync }:
  { lease: LeasedDatabase; exec?: TextCommandExecutor }): string {
  const database = lease.resources.database;
  if (!database || ['postgres', 'template0', 'template1'].includes(database)) {
    throw new Error('refusing to reset a PostgreSQL maintenance database');
  }
  const containerId = assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS, 'reset');
  // Owned backends give the application no CREATEDB privilege. Reset with the
  // controller's local administrator, preserving the application role/password.
  const admin = lease.resources.network ? 'postgres' : POSTGRES_USER;
  exec('docker', ['exec', containerId, 'dropdb', '-U', admin, '--if-exists', '--force', '--', database],
  { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  exec('docker', ['exec', containerId, 'createdb', '-U', admin,
    '--owner', POSTGRES_USER, '--template', 'template0', '--', database],
  { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  return `reset postgres database ${database}`;
}

export function provePostgresUse({ lease, marker, exec = execFileSync }:
  { lease: LeasedDatabase; marker: unknown; exec?: TextCommandExecutor }):
  { ok: boolean; verified: boolean; matches: number; reason: string } {
  // The container is checked only after the marker is.
  return provePostgresMarker(sql => psql(lease, exec, RESET_TIMEOUT_MS, 'database provenance')(sql), POSTGRES, marker);
}

export function setPostgresStock({ item, warehouse, quantity, lease, exec = execFileSync }: {
  item: string; warehouse: string; quantity: number; lease: LeasedDatabase;
  exec?: TextCommandExecutor;
}): { backend: string; item: string; warehouse: string; quantity: number } {
  return writePostgresStock(psql(lease, exec, WRITE_TIMEOUT_MS, 'direct database write'), POSTGRES,
    { item, warehouse, quantity });
}

export function preparePostgresDatabase({ lease, name, expectedName, wipe,
  exec = execFileSync }: {
  lease: LeasedDatabase; name: string; expectedName: string; wipe: boolean;
  exec?: TextCommandExecutor;
}): string {
  if (name !== expectedName) {
    throw new Error(`backend lease database ${name} does not match harness target ${expectedName}`);
  }
  if (name !== lease.resources.database) throw new Error('database target does not match its lease');
  if (wipe) {
    try {
      resetPostgres({ lease, exec });
    } catch (error) {
      throw new Error(`could not wipe ${name}: ${streams(error, 'message').split('\n')[0]}`, { cause: error });
    }
    return name;
  }
  const container = assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS,
    'database mutation');
  try {
    exec('docker', ['exec', container, 'psql', '-U', POSTGRES_USER, '-d', 'postgres',
      '-c', `CREATE DATABASE ${name} OWNER ${POSTGRES_USER};`],
    { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  } catch (error) {
    const exists = exec('docker', ['exec', container, 'psql', '-U', POSTGRES_USER,
      '-d', 'postgres', '-tAc', `SELECT 1 FROM pg_database WHERE datname = '${name}';`],
    { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS }).trim();
    if (exists !== '1') throw error;
  }
  return name;
}
