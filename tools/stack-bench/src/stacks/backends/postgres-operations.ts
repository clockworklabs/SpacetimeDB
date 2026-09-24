import { execFileSync } from 'node:child_process';
import { describesMissingStockInterface, stockInterfaceError, stockQuantity } from '../stock-interface.js';
import { checkoutStateSchema, verifyCheckoutSchema } from '../checkout-state.js';
import { orderDataColumns, orderDataError, readOrderDataSnapshot, type OrderDataStorage } from '../order-data.js';


import { assertLeasedContainer } from '../backend-reset-guard.js';
import type { LeasedDatabase } from '../backend-reset-guard.js';
import { POSTGRES_APPLICATION_IDENTITY } from '../hosted-database-identity.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';

const RESET_TIMEOUT_MS = 120_000;
const WRITE_TIMEOUT_MS = 60_000;
const sqlString = (value: unknown): string => `'${String(value).replaceAll("'", "''")}'`;

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object';

// A failed child process carries its output on the error.
const streams = (error: unknown, ...keys: readonly string[]): string =>
  record(error) ? keys.map(key => String(error[key] ?? '')).join('') : '';

const POSTGRES_USER = POSTGRES_APPLICATION_IDENTITY.user;

// Keep the declared foreign-key interface check; table and column names are fixed.
const STOCK_INTERFACE_SQL = `
WITH stock_interface AS (
  SELECT count(*) = 2 AND count(DISTINCT source_column.attname) = 2 AS valid
  FROM pg_constraint constraint_record
  JOIN pg_attribute source_column ON source_column.attrelid = constraint_record.conrelid
    AND source_column.attnum = constraint_record.conkey[1]
  JOIN pg_attribute target_column ON target_column.attrelid = constraint_record.confrelid
    AND target_column.attnum = constraint_record.confkey[1]
  WHERE constraint_record.contype = 'f'
    AND constraint_record.conrelid = 'public.stock'::regclass
    AND cardinality(constraint_record.conkey) = 1
    AND cardinality(constraint_record.confkey) = 1
    AND target_column.attname = 'id'
    AND ((source_column.attname = 'item_id' AND constraint_record.confrelid = 'public.item'::regclass)
      OR (source_column.attname = 'warehouse_id' AND constraint_record.confrelid = 'public.warehouse'::regclass))
)
`;

// The update applies only to one stock row under one named item and warehouse;
// the counts that follow name what was missing or ambiguous when it did not.
const stockUpdateSql = (itemName: string, warehouseName: string, quantity: number): string => {
  const item = sqlString(itemName), warehouse = sqlString(warehouseName);
  const items = `(SELECT count(*) FROM public.item WHERE name = ${item})`;
  const warehouses = `(SELECT count(*) FROM public.warehouse WHERE name = ${warehouse})`;
  return `
${STOCK_INTERFACE_SQL}
UPDATE public.stock SET quantity = ${quantity}
FROM public.item, public.warehouse
WHERE stock.item_id = item.id AND stock.warehouse_id = warehouse.id
  AND item.name = ${item} AND warehouse.name = ${warehouse}
  AND ${items} = 1 AND ${warehouses} = 1
  AND (SELECT count(*) FROM public.stock linked
    WHERE linked.item_id = item.id AND linked.warehouse_id = warehouse.id) = 1
  AND (SELECT valid FROM stock_interface);
${STOCK_INTERFACE_SQL}
SELECT json_build_object('items', ${items}, 'warehouses', ${warehouses},
  'stocks', (SELECT count(*) FROM public.stock JOIN public.item ON stock.item_id = item.id
    JOIN public.warehouse ON stock.warehouse_id = warehouse.id
    WHERE item.name = ${item} AND warehouse.name = ${warehouse} AND (SELECT valid FROM stock_interface)))::text;
`;
};

export function getPostgresCheckoutState({ account, item, app, lease, storage, exec = execFileSync }: {
  account: string; item: string; app: string; lease: LeasedDatabase; storage?: OrderDataStorage; exec?: TextCommandExecutor;
}) {
  if (storage?.kind === 'order-data') {
    const container = assertLeasedContainer(lease.resources.container, exec, WRITE_TIMEOUT_MS, 'order data read');
    // A single statement observes every table at one MVCC snapshot. Cast ids before JSON to retain bigint precision.
    const tables = Object.entries(orderDataColumns(storage)).map(([table, columns]) => `'${table}',
      COALESCE((SELECT json_agg(row) FROM (SELECT ${columns.map(column =>
        column === 'id' || column.endsWith('_id') ? `${column}::text AS ${column}` : column).join(',')}
        FROM public.${table}) row), '[]'::json)`);
    let output: string;
    try {
      output = exec('docker', ['exec', '-i', container, 'psql', '-U', POSTGRES_USER, '-d', lease.resources.database,
        '-v', 'ON_ERROR_STOP=1', '-At'], { encoding: 'utf8', input: `SELECT json_build_object(${tables.join(',')})::text;`,
        stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
    } catch (error) {
      if (describesMissingStockInterface(streams(error, 'stdout', 'stderr', 'message'))) {
        throw orderDataError('required order data table or column is missing', error);
      }
      throw error;
    }
    return readOrderDataSnapshot(JSON.parse(output.trim()), account, item, storage);
  }
  const schemaSha256 = verifyCheckoutSchema('postgres', app, ['server/src/schema.ts']);
  const container = assertLeasedContainer(lease.resources.container, exec, WRITE_TIMEOUT_MS, 'checkout state read');
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
  const output = exec('docker', ['exec', '-i', container,
    'psql', '-U', POSTGRES_USER, '-d', lease.resources.database, '-v', 'ON_ERROR_STOP=1', '-At'],
  { encoding: 'utf8', input: sql, stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  return { schemaSha256, state: checkoutStateSchema.parse(JSON.parse(output.trim())) };
}

export function getPostgresStock({ item, warehouse, lease, exec = execFileSync }: {
  item: string; warehouse?: string; lease: LeasedDatabase; exec?: TextCommandExecutor;
}): { backend: string; item: string; warehouse?: string; quantity: number } {
  const container = assertLeasedContainer(lease.resources.container, exec, WRITE_TIMEOUT_MS,
    'direct database read');
  const sql = `${STOCK_INTERFACE_SQL}
SELECT json_build_object(
  'items', (SELECT count(*) FROM public.item WHERE name = ${sqlString(item)}),
  'namedWarehouses', (SELECT count(*) FROM public.warehouse${warehouse === undefined ? '' : ` WHERE name = ${sqlString(warehouse)}`}),
  'warehouses', count(DISTINCT warehouse.id), 'quantities', COALESCE(json_agg(stock.quantity), '[]'::json))::text
FROM public.stock JOIN public.item ON stock.item_id = item.id
LEFT JOIN public.warehouse ON stock.warehouse_id = warehouse.id
WHERE item.name = ${sqlString(item)} ${warehouse === undefined ? '' : `AND warehouse.name = ${sqlString(warehouse)}`}
  AND (SELECT valid FROM stock_interface);`;
  let output: string;
  try {
    output = exec('docker', ['exec', '-i', container,
      'psql', '-U', POSTGRES_USER, '-d', lease.resources.database, '-v', 'ON_ERROR_STOP=1', '-At'],
    { encoding: 'utf8', input: sql, stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  } catch (error) {
    const detail = streams(error, 'stdout', 'stderr', 'message');
    if (describesMissingStockInterface(detail)) {
      throw stockInterfaceError(detail.trim().slice(-300), { cause: error });
    }
    throw error;
  }
  const rows = output.trim().split(/\r?\n/).filter(Boolean);
  if (rows.length !== 1) throw new Error(rows.length ? 'stock read is ambiguous: multiple relational stock interfaces match'
    : 'PostgreSQL stock read returned no result');
  const row: unknown = JSON.parse(rows[0]!);
  if (!record(row) || !Array.isArray(row.quantities)) {
    throw new Error('PostgreSQL stock read returned an invalid result');
  }
  if (row.quantities.length === 0) {
    const missing = `no stock data for ${item}${warehouse === undefined ? '' : ` / ${warehouse}`}`;
    if (row.items === 0) throw stockInterfaceError(missing, { missingRow: 'item' });
    if (row.namedWarehouses === 0) throw stockInterfaceError(missing, { missingRow: 'warehouse' });
    if (row.items !== 1 || (warehouse !== undefined && row.namedWarehouses !== 1)) {
      throw stockInterfaceError('stock read is ambiguous: duplicate item or warehouse rows', { invalid: true });
    }
    throw stockInterfaceError(missing, { missingRow: 'stock' });
  }
  if (row.items !== 1 || row.warehouses !== row.quantities.length
    || (warehouse !== undefined && (row.namedWarehouses !== 1 || row.quantities.length !== 1))) {
    throw stockInterfaceError('stock read is ambiguous: duplicate or missing item/warehouse links', { invalid: true });
  }
  const quantity = row.quantities.reduce((sum: number, value: unknown) =>
    stockQuantity(sum + stockQuantity(value)), 0);
  return { backend: 'postgres', item, ...(warehouse === undefined ? {} : { warehouse }), quantity };
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
  if (typeof marker !== 'string' || !marker) {
    throw new Error('PostgreSQL provenance requires a non-empty application marker');
  }
  const containerId = assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS,
    'database provenance');
  const sql = "SELECT format('SELECT %L WHERE EXISTS (SELECT 1 FROM %I.%I WHERE %I::text = %L LIMIT 1);', "
    + "table_schema || '.' || table_name || '.' || column_name, table_schema, table_name, "
    + `column_name, ${sqlString(marker)}) FROM information_schema.columns `
    + "WHERE table_schema = 'public' ORDER BY table_name, ordinal_position\n\\gexec\n";
  const output = exec('docker', ['exec', '-i', containerId,
    'psql', '-U', POSTGRES_USER, '-d', lease.resources.database,
    '-v', 'ON_ERROR_STOP=1', '-At'],
  { encoding: 'utf8', input: sql, stdio: 'pipe', timeout: RESET_TIMEOUT_MS }).trim();
  const matches = output ? output.split(/\r?\n/).filter(Boolean) : [];
  return { ok: matches.length > 0, verified: true, matches: matches.length,
    reason: matches.length
      ? 'the application marker exists in the leased PostgreSQL database'
      : 'the application marker is absent from the leased PostgreSQL database' };
}

export function setPostgresStock({ item, warehouse, quantity, lease, exec = execFileSync }: {
  item: string; warehouse: string; quantity: number; lease: LeasedDatabase;
  exec?: TextCommandExecutor;
}): { backend: string; item: string; warehouse: string; quantity: number } {
  const container = assertLeasedContainer(lease.resources.container, exec, WRITE_TIMEOUT_MS,
    'direct database write');
  const dbName = lease.resources.database;
  let output: string;
  try {
    output = exec('docker', ['exec', '-i', container,
      'psql', '-U', POSTGRES_USER, '-d', dbName, '-v', 'ON_ERROR_STOP=1', '-At'],
    { encoding: 'utf8', input: stockUpdateSql(item, warehouse, quantity),
      stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  } catch (error) {
    // psql refusing the statement over an absent table or column is the
    // application not providing the interface, not a harness fault.
    const detail = streams(error, 'stdout', 'stderr', 'message');
    if (describesMissingStockInterface(detail)) {
      throw stockInterfaceError(detail.trim().slice(-300), { cause: error });
    }
    throw error;
  }
  if ((output.match(/UPDATE 1\b/g) ?? []).length === 1) {
    return { backend: 'postgres', item, warehouse, quantity };
  }
  let counts: unknown;
  try { counts = JSON.parse(output.trim().split(/\r?\n/).find(line => line.startsWith('{')) ?? ''); }
  catch { throw new Error('PostgreSQL stock write returned an invalid result'); }
  if (!record(counts) || !['items', 'warehouses', 'stocks'].every(key => Number.isSafeInteger(counts[key]))) {
    throw new Error('PostgreSQL stock write returned an invalid result');
  }
  if (counts.items === 0) throw stockInterfaceError(`required item ${item} is absent`, { missingRow: 'item' });
  if (counts.warehouses === 0) throw stockInterfaceError(`required warehouse ${warehouse} is absent`, { missingRow: 'warehouse' });
  if (counts.items !== 1 || counts.warehouses !== 1 || (counts.stocks as number) > 1) {
    throw stockInterfaceError('stock write is ambiguous: duplicate item, warehouse, or stock rows', { invalid: true });
  }
  throw stockInterfaceError(`could not locate one relational stock row for ${item} / ${warehouse}`, { missingRow: 'stock' });
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
