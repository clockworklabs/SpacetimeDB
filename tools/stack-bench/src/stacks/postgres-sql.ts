import { describesMissingStockInterface, stockInterfaceError, stockQuantity } from './stock-interface.js';
import { orderDataColumns, orderDataError, readOrderDataSnapshot, type OrderDataStorage } from './order-data.js';

// The relational stock, order-data and provenance reads of any stack whose
// application data is PostgreSQL. Each stack runs the SQL through its own psql.
export type PsqlRunner = (sql: string) => string;
// `quiet` psql prints no command tag, so its stock update reports its own rows.
export interface PostgresStack { backend: string; label: string; quiet: boolean }

const sqlString = (value: unknown): string => `'${String(value).replaceAll("'", "''")}'`;

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object';

// A failed child process carries its output on the error.
export const streams = (error: unknown, ...keys: readonly string[]): string =>
  record(error) ? keys.map(key => String(error[key] ?? '')).join('') : '';

// psql refusing a statement over an absent table or column is the application
// not providing the interface, not a harness fault.
function stockFailure(error: unknown): unknown {
  const detail = streams(error, 'stdout', 'stderr', 'message');
  return describesMissingStockInterface(detail) ? stockInterfaceError(detail.trim().slice(-300), { cause: error }) : error;
}

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
function stockUpdateSql(itemName: string, warehouseName: string, quantity: number, quiet: boolean): string {
  const item = sqlString(itemName), warehouse = sqlString(warehouseName);
  const items = `(SELECT count(*) FROM public.item WHERE name = ${item})`;
  const warehouses = `(SELECT count(*) FROM public.warehouse WHERE name = ${warehouse})`;
  const update = `UPDATE public.stock SET quantity = ${quantity}
FROM public.item, public.warehouse
WHERE stock.item_id = item.id AND stock.warehouse_id = warehouse.id
  AND item.name = ${item} AND warehouse.name = ${warehouse}
  AND ${items} = 1 AND ${warehouses} = 1
  AND (SELECT count(*) FROM public.stock linked
    WHERE linked.item_id = item.id AND linked.warehouse_id = warehouse.id) = 1
  AND (SELECT valid FROM stock_interface)`;
  const counts = `'items', ${items}, 'warehouses', ${warehouses},
  'stocks', (SELECT count(*) FROM public.stock JOIN public.item ON stock.item_id = item.id
    JOIN public.warehouse ON stock.warehouse_id = warehouse.id
    WHERE item.name = ${item} AND warehouse.name = ${warehouse} AND (SELECT valid FROM stock_interface)))::text;
`;
  return quiet
    ? `${STOCK_INTERFACE_SQL.trimEnd()}, updated AS (\n${update}\nRETURNING 1)\n`
      + `SELECT json_build_object('updated', (SELECT count(*) FROM updated), ${counts}`
    : `\n${STOCK_INTERFACE_SQL}\n${update};\n${STOCK_INTERFACE_SQL}\nSELECT json_build_object(${counts}`;
}

export function readPostgresStock(run: PsqlRunner, { backend, label }: PostgresStack, item: string, warehouse?: string):
  { backend: string; item: string; warehouse?: string; quantity: number } {
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
    output = run(sql);
  } catch (error) {
    throw stockFailure(error);
  }
  const rows = output.trim().split(/\r?\n/).filter(Boolean);
  if (rows.length !== 1) throw new Error(rows.length ? 'stock read is ambiguous: multiple relational stock interfaces match'
    : `${label} stock read returned no result`);
  const row: unknown = JSON.parse(rows[0]!);
  if (!record(row) || !Array.isArray(row.quantities)) {
    throw new Error(`${label} stock read returned an invalid result`);
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
  return { backend, item, ...(warehouse === undefined ? {} : { warehouse }), quantity };
}

export function writePostgresStock(run: PsqlRunner, { backend, label, quiet }: PostgresStack,
  { item, warehouse, quantity }: { item: string; warehouse: string; quantity: number }):
  { backend: string; item: string; warehouse: string; quantity: number } {
  let output: string;
  try {
    output = run(stockUpdateSql(item, warehouse, quantity, quiet));
  } catch (error) {
    throw stockFailure(error);
  }
  const written = { backend, item, warehouse, quantity };
  if (!quiet && (output.match(/UPDATE 1\b/g) ?? []).length === 1) return written;
  const invalid = () => new Error(`${label} stock write returned an invalid result`);
  const rows = output.trim().split(/\r?\n/).filter(Boolean);
  let counts: unknown;
  try { counts = quiet ? (rows.length === 1 ? JSON.parse(rows[0]!) : undefined) : JSON.parse(rows.find(line => line.startsWith('{')) ?? ''); }
  catch { throw invalid(); }
  const keys = [...(quiet ? ['updated'] : []), 'items', 'warehouses', 'stocks'];
  if (!record(counts) || !keys.every(key => Number.isSafeInteger(counts[key]))) throw invalid();
  if (counts.updated === 1) return written;
  if (counts.items === 0) throw stockInterfaceError(`required item ${item} is absent`, { missingRow: 'item' });
  if (counts.warehouses === 0) throw stockInterfaceError(`required warehouse ${warehouse} is absent`, { missingRow: 'warehouse' });
  if (counts.items !== 1 || counts.warehouses !== 1 || (counts.stocks as number) > 1) {
    throw stockInterfaceError('stock write is ambiguous: duplicate item, warehouse, or stock rows', { invalid: true });
  }
  throw stockInterfaceError(`could not locate one relational stock row for ${item} / ${warehouse}`, { missingRow: 'stock' });
}

// A single statement observes every table at one MVCC snapshot. Cast ids before JSON to retain bigint precision.
export function readPostgresOrderData(run: PsqlRunner, account: string, item: string, storage: OrderDataStorage) {
  const tables = Object.entries(orderDataColumns(storage)).map(([table, columns]) => `'${table}',
      COALESCE((SELECT json_agg(row) FROM (SELECT ${columns.map(column =>
        column === 'id' || column.endsWith('_id') ? `${column}::text AS ${column}` : column).join(',')}
        FROM public.${table}) row), '[]'::json)`);
  let output: string;
  try {
    output = run(`SELECT json_build_object(${tables.join(',')})::text;`);
  } catch (error) {
    if (describesMissingStockInterface(streams(error, 'stdout', 'stderr', 'message'))) {
      throw orderDataError('required order data table or column is missing', error);
    }
    throw error;
  }
  return readOrderDataSnapshot(JSON.parse(output.trim()), account, item, storage);
}

// Application data lives in `public`.
export function provePostgresMarker(run: PsqlRunner, { label }: PostgresStack, marker: unknown):
  { ok: boolean; verified: boolean; matches: number; reason: string } {
  if (typeof marker !== 'string' || !marker) {
    throw new Error(`${label} provenance requires a non-empty application marker`);
  }
  const output = run("SELECT format('SELECT %L WHERE EXISTS (SELECT 1 FROM %I.%I WHERE %I::text = %L LIMIT 1);', "
    + "table_schema || '.' || table_name || '.' || column_name, table_schema, table_name, "
    + `column_name, ${sqlString(marker)}) FROM information_schema.columns `
    + "WHERE table_schema = 'public' ORDER BY table_name, ordinal_position\n\\gexec\n").trim();
  const matches = output ? output.split(/\r?\n/).filter(Boolean).length : 0;
  return { ok: matches > 0, verified: true, matches,
    reason: matches
      ? `the application marker exists in the leased ${label} database`
      : `the application marker is absent from the leased ${label} database` };
}
