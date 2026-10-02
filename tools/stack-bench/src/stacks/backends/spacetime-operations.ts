import { execFileSync } from 'node:child_process';
import { describesMissingStockInterface, stockInterfaceError, stockQuantity } from '../stock-interface.js';
import { checkoutId, checkoutMinor, checkoutStateSchema, verifyCheckoutSchema } from '../checkout-state.js';
import { orderDataColumns, orderDataError, readOrderDataSnapshot, type OrderDataStorage } from '../order-data.js';
import { assertLeasedContainer } from '../backend-reset-guard.js';

import { leasedSpacetimeTarget } from '../../runtime/spacetime-target.js';
import { resolveSpacetimeModuleLayout } from '../../runtime/spacetime-layout.js';
import { CODING_CONTAINER_SPACETIME_CLI, codingContainerAgentCommand, codingContainerAgentExecOptions }
  from '../../runtime/coding-container-policy.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';

const RESET_TIMEOUT_MS = 120_000;
const WRITE_TIMEOUT_MS = 60_000;
const sqlString = (value: unknown): string => `'${String(value).replaceAll("'", "''")}'`;
const sqlIdentifier = (value: unknown): string => {
  const name = String(value);
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) {
    throw new Error(`SpacetimeDB schema contains an unsupported identifier: ${name}`);
  }
  return name;
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object';

// A failed child process carries its output on the error.
const streams = (error: unknown, ...keys: readonly string[]): string =>
  record(error) ? keys.map(key => String(error[key] ?? '')).join('') : '';

type SpacetimeLease = { resources: { module: string; serverUri: string } };
const agentExec = () => ['exec', ...codingContainerAgentExecOptions()];

function stringColumns(schema: unknown): Array<{ table: string; column: string }> {
  if (!record(schema) || !Array.isArray(schema.sections)) {
    throw new Error('SpacetimeDB describe returned an invalid schema');
  }
  const typespace = schema.sections.find(section => record(section) && record(section.Typespace));
  const tablesSection = schema.sections.find(section => record(section) && Array.isArray(section.Tables));
  const types = record(typespace) && record(typespace.Typespace) && Array.isArray(typespace.Typespace.types)
    ? typespace.Typespace.types : null;
  const tables = record(tablesSection) && Array.isArray(tablesSection.Tables) ? tablesSection.Tables : null;
  if (!types || !tables) throw new Error('SpacetimeDB describe omitted tables or types');
  const namesSection = schema.sections.find(section => record(section) && 'ExplicitNames' in section);
  const names = new Map<string, string>();
  if (namesSection !== undefined) {
    if (!record(namesSection) || !record(namesSection.ExplicitNames)
      || !Array.isArray(namesSection.ExplicitNames.entries)) throw new Error('SpacetimeDB describe returned invalid explicit names');
    for (const entry of namesSection.ExplicitNames.entries) {
      if (!record(entry)) throw new Error('SpacetimeDB describe returned an invalid name entry');
      if (!('Table' in entry)) continue;
      const name = entry.Table;
      if (!record(name) || typeof name.source_name !== 'string' || !name.source_name
        || typeof name.canonical_name !== 'string' || !name.canonical_name || names.has(name.source_name)) {
        throw new Error('SpacetimeDB describe returned an invalid table name');
      }
      names.set(name.source_name, name.canonical_name);
    }
  }
  const columns: Array<{ table: string; column: string }> = [];
  for (const table of tables) {
    if (!record(table) || typeof table.source_name !== 'string'
      || !Number.isInteger(table.product_type_ref)) continue;
    const type = types[Number(table.product_type_ref)];
    const elements = record(type) && record(type.Product) && Array.isArray(type.Product.elements)
      ? type.Product.elements : [];
    for (const element of elements) {
      if (!record(element) || !record(element.name) || typeof element.name.some !== 'string'
        || !record(element.algebraic_type) || !('String' in element.algebraic_type)) continue;
      columns.push({ table: sqlIdentifier(names.get(table.source_name) ?? table.source_name), column: sqlIdentifier(element.name.some) });
    }
  }
  return columns;
}

export function proveSpacetimeUse({ lease, marker, exec = execFileSync }:
  { lease: SpacetimeLease; marker: unknown; exec?: TextCommandExecutor }):
  { ok: boolean; verified: boolean; matches: number; reason: string } {
  if (typeof marker !== 'string' || !marker) {
    throw new Error('SpacetimeDB provenance requires a non-empty application marker');
  }
  const target = leasedSpacetimeTarget({ requireBuildContainer: true, exec });
  const container = target.buildContainer;
  if (!container) throw new Error('leased build container is not active');
  if (target.mod !== lease.resources.module || target.uri !== lease.resources.serverUri) {
    throw new Error('SpacetimeDB provenance target does not match the authenticated lease');
  }
  const run = (args: string[]): string => exec('docker', [...agentExec(), container.id,
    ...codingContainerAgentCommand(CODING_CONTAINER_SPACETIME_CLI, args)],
  { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  const schema = JSON.parse(run(['describe', '--json', target.mod, '-s', target.containerUri]));
  let matches = 0;
  for (const { table, column } of stringColumns(schema)) {
    const output = run(['sql', target.mod, '-s', target.containerUri,
      `select ${column} from ${table} where ${column} = ${sqlString(marker)}`]);
    if (output.includes(`"${marker}"`)) matches += 1;
  }
  return { ok: matches > 0, verified: true, matches,
    reason: matches
      ? 'the application marker exists in the leased SpacetimeDB module'
      : 'the application marker is absent from the leased SpacetimeDB module' };
}

export function resetSpacetime({ lease, app, exec = execFileSync }:
  { lease: SpacetimeLease; app: string; exec?: TextCommandExecutor }): string {
  const layout = resolveSpacetimeModuleLayout(app);
  const target = leasedSpacetimeTarget({ requireBuildContainer: true, exec });
  const container = target.buildContainer;
  if (!container) throw new Error('leased build container is not active');
  const containerModule = layout.containerPath;
  try {
    exec('docker', [...agentExec(), container.id,
      ...codingContainerAgentCommand('test', ['-d', containerModule])],
      { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
    exec('docker', [...agentExec(), '-w', containerModule, container.id,
      ...codingContainerAgentCommand(CODING_CONTAINER_SPACETIME_CLI,
        ['publish', lease.resources.module, '--module-path', containerModule,
          '-s', target.containerUri, '--delete-data', '-y'])],
    { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  } catch (error) {
    const detail = streams(error, 'stdout', 'stderr').trim() || streams(error, 'message');
    throw new Error(`SpacetimeDB reset publish failed in leased container: ${detail}`, { cause: error });
  }
  return `reset spacetime module ${lease.resources.module} on ${lease.resources.serverUri}`;
}

export function setSpacetimeStock({ item, warehouse, quantity, spacetime,
  exec = execFileSync }: {
  item: string; warehouse: string; quantity: number; exec?: TextCommandExecutor;
  spacetime?: { buildContainer?: { id: string; name: string } | null; mod: string; containerUri: string };
}): { backend: string; item: string; warehouse: string; quantity: number } {
  const container = spacetime?.buildContainer;
  if (!container) {
    throw new Error('SpacetimeDB build container is unavailable for direct SQL');
  }
  // Distinguish an absent application row from an unsuccessful harness write.
  // Reuse the strict reader so malformed CLI output is never treated as absence.
  getSpacetimeStock({ item, warehouse, spacetime, exec });
  const query = (sql: string): string => exec('docker', [...agentExec(), container.id,
    ...codingContainerAgentCommand(CODING_CONTAINER_SPACETIME_CLI,
      ['sql', spacetime.mod, '-s', spacetime.containerUri, sql])],
  { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  const idOf = (table: 'item' | 'warehouse', name: string): string => {
    const output = guarded(`select id from ${table} where name = ${sqlString(name)}`);
    const match = output.match(/^\s*(\d+)\s*$/m);
    if (!match?.[1]) throw stockInterfaceError(`no ${table} named "${name}"`, { missingRow: table });
    return match[1];
  };
  // A missing or private table, or a missing column, is the application not
  // providing the stock data interface its contract names, not a harness fault.
  const guarded = (sql: string): string => {
    try { return query(sql); } catch (error) {
      const detail = streams(error, 'stdout', 'stderr', 'message');
      if (describesMissingStockInterface(detail)) throw stockInterfaceError(detail.trim().slice(-300));
      throw error;
    }
  };
  const itemId = idOf('item', item);
  const warehouseId = idOf('warehouse', warehouse);
  guarded(`update stock set quantity = ${quantity} where item_id = ${itemId} and warehouse_id = ${warehouseId}`);
  const verified = guarded(`select quantity from stock where item_id = ${itemId} and warehouse_id = ${warehouseId}`);
  if (!new RegExp(`^\\s*${quantity}\\s*$`, 'm').test(verified)) {
    throw new Error(`stock row for ${item} / ${warehouse} did not update to ${quantity}`);
  }
  return { backend: 'spacetime', item, warehouse, quantity };
}

export function getSpacetimeStock({ item, warehouse, spacetime, exec = execFileSync }: {
  item: string; warehouse?: string; exec?: TextCommandExecutor;
  spacetime?: { buildContainer?: { id: string; name: string } | null; mod: string; containerUri: string };
}): { backend: string; item: string; warehouse?: string; quantity: number } {
  if (!spacetime?.buildContainer) throw new Error('SpacetimeDB build container is unavailable for direct SQL');
  const container = assertLeasedContainer(spacetime.buildContainer, exec, WRITE_TIMEOUT_MS,
    'direct database read');
  const query = (sql: string, columns: string[]): string[][] => {
    let output: string;
    try {
      output = exec('docker', [...agentExec(), container,
        ...codingContainerAgentCommand(CODING_CONTAINER_SPACETIME_CLI,
          ['sql', spacetime.mod, '-s', spacetime.containerUri, sql])],
      { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
    } catch (error) {
      const detail = streams(error, 'stdout', 'stderr', 'message');
      if (describesMissingStockInterface(detail)) throw stockInterfaceError(detail.trim().slice(-300));
      throw error;
    }
    const lines = output.trim().split(/\r?\n/).map(line => line.trim());
    const cells = (line: string): string[] => line.split('|').map(cell => cell.trim());
    if (JSON.stringify(cells(lines[0] ?? '')) !== JSON.stringify(columns)
      || !/^[-+\s]+$/.test(lines[1] ?? '')) {
      throw new Error('SpacetimeDB stock read returned an invalid table header');
    }
    return lines.slice(2).map(line => {
      const row = cells(line);
      if (row.length !== columns.length || row.some(cell => !/^-?\d+$/.test(cell))) {
        throw stockInterfaceError('stock interface contains invalid numeric data', { invalid: true });
      }
      return row;
    });
  };
  const idOf = (table: 'item' | 'warehouse', name: string): string => {
    const rows = query(`select id from ${table} where name = ${sqlString(name)}`, ['id']);
    if (!rows.length) throw stockInterfaceError(`required ${table} is missing`, { missingRow: table });
    if (rows.length !== 1) throw stockInterfaceError(`required ${table} is ambiguous`, { invalid: true });
    const id = rows[0]![0]!;
    if (!/^\d+$/.test(id)) throw stockInterfaceError(`${table} contains an invalid id`, { invalid: true });
    return id;
  };
  const itemId = idOf('item', item);
  const warehouseId = warehouse === undefined ? undefined : idOf('warehouse', warehouse);
  const parents = warehouseId === undefined ? query('select id from warehouse', ['id']).map(row => row[0]!) : [warehouseId];
  if (!parents.length || parents.some(id => !/^\d+$/.test(id)) || new Set(parents).size !== parents.length) {
    throw stockInterfaceError('warehouse ids are missing or ambiguous', { missingRow: 'warehouse' });
  }
  const rows = query(`select warehouse_id, quantity from stock where item_id = ${itemId}`
    + (warehouseId === undefined ? '' : ` and warehouse_id = ${warehouseId}`), ['warehouse_id', 'quantity']);
  if (!rows.length) throw stockInterfaceError('required stock row is absent', { missingRow: 'stock' });
  const seen = new Set<string>();
  let quantity = 0;
  for (const [id, value] of rows) {
    if (!id || parents.filter(parent => parent === id).length !== 1 || seen.has(id)) {
      throw stockInterfaceError('stock warehouse links are invalid or duplicated', { invalid: true });
    }
    seen.add(id);
    quantity = stockQuantity(quantity + stockQuantity(Number(value)));
  }
  return { backend: 'spacetime', item, ...(warehouse === undefined ? {} : { warehouse }), quantity };
}

export function getSpacetimeCheckoutState({ account, item, app, spacetime, storage, exec = execFileSync }: {
  account: string; item: string; app: string; storage?: OrderDataStorage; exec?: TextCommandExecutor;
  spacetime?: { buildContainer?: { id: string; name: string } | null; mod: string; containerUri: string };
}) {
  if (storage?.kind === 'order-data') {
    if (!spacetime?.buildContainer) throw new Error('SpacetimeDB build container is unavailable for order data');
    const container = assertLeasedContainer(spacetime.buildContainer, exec, WRITE_TIMEOUT_MS, 'order data read');
    const tables = Object.keys(orderDataColumns(storage));
    let output: string;
    try {
      output = exec('docker', [...agentExec(), container,
        ...codingContainerAgentCommand(CODING_CONTAINER_SPACETIME_CLI, ['subscribe', spacetime.mod,
          '-s', spacetime.containerUri, '--print-initial-update', '--num-updates', '0', '--timeout', '30',
          ...tables.map(table => `SELECT * FROM ${table}`)])],
        { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
    } catch (error) {
      if (describesMissingStockInterface(streams(error, 'stdout', 'stderr', 'message'))) {
        throw orderDataError('required order data table is absent or unreadable', error);
      }
      throw error;
    }
    const snapshot: unknown = JSON.parse(output.trim(), (key: string, value: unknown, context?: { source?: string }) => {
      if ((key === 'id' || key.endsWith('_id')) && typeof value === 'number' && !Number.isSafeInteger(value)) {
        // The native CLI emits u64 ids as JSON numbers. Recover the original token, not its rounded JS value.
        if (!context?.source || !/^\d+$/.test(context.source)) throw new Error('native order snapshot cannot preserve an exact identifier');
        return context.source;
      }
      return value;
    });
    if (!record(snapshot) || Object.keys(snapshot).some(key => !tables.includes(key))) {
      throw new Error('order subscription returned an invalid snapshot');
    }
    const rows = Object.fromEntries(tables.map(table => {
      const value = snapshot[table];
      if (value === undefined) return [table, []]; // Native initial updates omit empty tables.
      if (!record(value) || !Array.isArray(value.inserts) || !Array.isArray(value.deletes) || value.deletes.length) {
        throw new Error('order subscription returned an invalid initial table');
      }
      return [table, value.inserts];
    }));
    return readOrderDataSnapshot(rows, account, item, storage);
  }
  const schemaSha256 = verifyCheckoutSchema('spacetime', app, ['backend/spacetimedb/src/schema.ts']);
  if (!spacetime?.buildContainer) throw new Error('SpacetimeDB build container is unavailable for checkout snapshot');
  const container = assertLeasedContainer(spacetime.buildContainer, exec, WRITE_TIMEOUT_MS, 'checkout state read');
  const selections = [
    ['account', 'id', `username=${sqlString(account)}`],
    ['item', 'id,price', `name=${sqlString(item)}`],
    ['cart_item', 'account_id,item_id,quantity'],
    ['stock', 'item_id,warehouse_id,quantity'],
    ['reservation', 'account_id,item_id,warehouse_id,quantity'],
    ['customer_order', 'id,account_id,total,status'],
    ['order_item', 'id,order_id,item_id,quantity,unit_price'],
    ['payment_record', 'id,order_id,amount,status'],
    ['order_item_stock', 'order_item_id,warehouse_id,quantity'],
  ] as const;
  // One initial subscription snapshot covers every query at the same database
  // state. The SQL endpoint accepts only one statement per request.
  const queries = selections.map(([table, , where]) => `SELECT * FROM ${table}${where ? ` WHERE ${where}` : ''}`);
  const output = exec('docker', [...agentExec(), container,
    ...codingContainerAgentCommand(CODING_CONTAINER_SPACETIME_CLI,
      ['subscribe', spacetime.mod, '-s', spacetime.containerUri, '--print-initial-update', '--num-updates', '0', '--timeout', '30', ...queries])],
    { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  const results: unknown = JSON.parse(output.trim());
  if (!record(results) || Object.keys(results).some(key => !selections.some(([table]) => table === key))) {
    throw new Error('checkout subscription returned an invalid snapshot');
  }
  const rows = selections.map(([table, columns]): unknown[][] => {
    // A successful subscription acknowledges all queries, but empty tables may
    // be omitted. Unknown tables fail the command; source mapping is checked above.
    if (!(table in results)) return [];
    const result = results[table];
    if (!record(result) || !Array.isArray(result.inserts) || !Array.isArray(result.deletes) || result.deletes.length) {
      throw new Error('checkout subscription returned an invalid initial table');
    }
    return result.inserts.map(row => {
      if (!record(row) || columns.split(',').some(column => !(column in row))) {
        throw new Error(`checkout subscription returned an invalid row shape for ${table}: ${record(row) ? Object.keys(row).join(',') : 'not an object'}`);
      }
      return columns.split(',').map(column => row[column]);
    });
  });
  if (rows[0]!.length !== 1 || rows[1]!.length !== 1) throw new Error('checkout account or item is missing or ambiguous');
  const accountId = checkoutId(rows[0]![0]![0]);
  const itemId = checkoutId(rows[1]![0]![0]);
  return { schemaSha256, state: checkoutStateSchema.parse({
    accountId, itemId, priceMinor: checkoutMinor(rows[1]![0]![1]),
    cart: rows[2]!.filter(row => checkoutId(row[0]) === accountId).map(row => ({ itemId: checkoutId(row[1]), quantity: row[2] })),
    stock: rows[3]!.filter(row => checkoutId(row[0]) === itemId).map(row => ({ warehouseId: checkoutId(row[1]), quantity: row[2] })),
    reservations: rows[4]!.filter(row => checkoutId(row[0]) === accountId)
      .map(row => ({ itemId: checkoutId(row[1]), warehouseId: checkoutId(row[2]), quantity: row[3] })),
    orders: rows[5]!.map(row => ({ id: checkoutId(row[0]), accountId: checkoutId(row[1]), totalMinor: checkoutMinor(row[2]), status: row[3],
      lines: rows[6]!.filter(line => checkoutId(line[1]) === checkoutId(row[0]))
        .map(line => ({ itemId: checkoutId(line[2]), quantity: line[3], priceMinor: checkoutMinor(line[4]),
          allocations: rows[8]!.filter(allocation => checkoutId(allocation[0]) === checkoutId(line[0]))
            .map(allocation => ({ warehouseId: checkoutId(allocation[1]), quantity: allocation[2] })) })) })),
    payments: rows[7]!.map(row => ({ id: checkoutId(row[0]), orderId: checkoutId(row[1]), amountMinor: checkoutMinor(row[2]), status: row[3] })),
    orphanOrderLines: rows[6]!.filter(line => !rows[5]!.some(order => checkoutId(order[0]) === checkoutId(line[1]))).length,
  }) };
}

export function prepareSpacetimeDatabase({ lease, name, wipe, exec = execFileSync,
  cli, expectedServerUri, expectedModule }: {
  lease: SpacetimeLease; name: string; wipe: boolean; exec?: TextCommandExecutor;
  cli: string; expectedServerUri: string; expectedModule: string;
}): string {
  if (lease.resources.serverUri !== expectedServerUri
    || lease.resources.module !== expectedModule) {
    throw new Error('SpacetimeDB lease target does not match the harness run');
  }
  if (!wipe) return name;
  try {
    exec(cli, ['delete', lease.resources.module, '-s', lease.resources.serverUri, '-y'],
      { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
    console.error(`  deleted module ${lease.resources.module} — a build starts with none published`);
  } catch (error) {
    const detail = streams(error, 'stdout', 'stderr', 'message');
    if (!/(?:404\s+Not Found|no such database|failed to find database)/i.test(detail)) {
      throw new Error(`could not delete prior module ${lease.resources.module}: ${detail.trim().split('\n')[0]}`,
        { cause: error });
    }
  }
  return name;
}
