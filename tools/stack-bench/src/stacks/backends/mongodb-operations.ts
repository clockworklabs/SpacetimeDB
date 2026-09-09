import { execFileSync } from 'node:child_process';
import { stockInterfaceError, stockQuantity } from '../stock-interface.js';

import { assertLeasedContainer } from '../backend-reset-guard.js';
import type { LeasedDatabase } from '../backend-reset-guard.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';
import { attemptDatabaseIdentity } from '../hosted-database-identity.js';

function mongoShell(lease: LeasedDatabase): string[] {
  const database = lease.resources.database;
  if (!lease.resources.network) return ['mongosh', database];
  const { user, password } = attemptDatabaseIdentity(lease.ownershipToken ?? '');
  return ['mongosh', database, '--username', user, '--password', password,
    '--authenticationDatabase', lease.resources.database];
}

const RESET_TIMEOUT_MS = 120_000;
const WRITE_TIMEOUT_MS = 60_000;

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object';

// A failed child process carries its output on the error.
const streams = (error: unknown, ...keys: readonly string[]): string =>
  record(error) ? keys.map(key => String(error[key] ?? '')).join('') : '';

export function resetMongoDb({ lease, exec = execFileSync }:
  { lease: LeasedDatabase; exec?: TextCommandExecutor }): string {
  const containerId = assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS, 'reset');
  const authentication = lease.resources.network
    ? ['--username', 'admin', '--password', attemptDatabaseIdentity(lease.ownershipToken ?? '').adminPassword,
      '--authenticationDatabase', 'admin'] : [];
  exec('docker', ['exec', containerId, 'mongosh', lease.resources.database, ...authentication,
    '--quiet', '--eval', 'db.dropDatabase()'],
  { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  return `reset mongodb database ${lease.resources.database}`;
}

export function proveMongoDbUse({ lease, marker, exec = execFileSync }:
  { lease: LeasedDatabase; marker: unknown; exec?: TextCommandExecutor }):
  { ok: boolean; verified: boolean; matches: number; reason: string } {
  if (typeof marker !== 'string' || !marker) {
    throw new Error('MongoDB provenance requires a non-empty application marker');
  }
  const containerId = assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS,
    'database provenance');
  const script = `const marker = ${JSON.stringify(marker)};
function containsMarker(value) {
  if (value === marker) return true;
  if (!value || typeof value !== 'object') return false;
  if (Array.isArray(value)) return value.some(containsMarker);
  return Object.values(value).some(containsMarker);
}
let matches = 0;
for (const name of db.getCollectionNames()) {
  const cursor = db.getCollection(name).find();
  while (cursor.hasNext()) {
    if (containsMarker(cursor.next())) { matches += 1; break; }
  }
}
print(matches);`;
  const output = exec('docker', ['exec', containerId,
    ...mongoShell(lease), '--quiet', '--eval', script],
  { encoding: 'utf8', stdio: 'pipe', timeout: RESET_TIMEOUT_MS }).trim();
  const matches = Number(output.split(/\r?\n/).at(-1));
  if (!Number.isSafeInteger(matches) || matches < 0) {
    throw new Error(`MongoDB provenance returned an invalid count: ${output.slice(-120)}`);
  }
  return { ok: matches > 0, verified: true, matches,
    reason: matches
      ? 'the application marker exists in the leased MongoDB database'
      : 'the application marker is absent from the leased MongoDB database' };
}

export function setMongoDbStock({ item, warehouse, quantity, lease, exec = execFileSync }: {
  item: string; warehouse: string; quantity: number; lease: LeasedDatabase;
  exec?: TextCommandExecutor;
}): { backend: string; item: string; warehouse: string; quantity: number } {
  const container = assertLeasedContainer(lease.resources.container, exec, WRITE_TIMEOUT_MS,
    'direct database write');
  const script = `
    const collections = db.getCollectionNames();
    if (!['item', 'warehouse', 'stock'].every(name => collections.includes(name))) { print('MISSING'); quit(1); }
    const items = db.item.find({ name: ${JSON.stringify(item)} }).limit(2).toArray();
    const warehouses = db.warehouse.find({ name: ${JSON.stringify(warehouse)} }).limit(2).toArray();
    if (!items.length || !warehouses.length) { print(!items.length ? 'MISSING_ITEM' : 'MISSING_WAREHOUSE'); quit(1); }
    if (items.length !== 1 || warehouses.length !== 1) { print('AMBIGUOUS_PARENT'); quit(1); }
    const it = items[0], wh = warehouses[0];
    function references(id) {
      if (id && id._bsontype === 'ObjectId') return [id, id.toHexString()];
      if (typeof id === 'string' && /^[0-9a-f]{24}$/.test(id)) return [id, new ObjectId(id)];
      return [id];
    }
    const iid = references(it.id ?? it._id), wid = references(wh.id ?? wh._id);
    const matches = db.stock.find({ $or: [
      { item_id: { $in: iid }, warehouse_id: { $in: wid } },
      { itemId: { $in: iid }, warehouseId: { $in: wid } }
    ] }).limit(2).toArray();
    if (!matches.length) { print('NOMATCH'); quit(0); }
    if (matches.length !== 1) { print('AMBIGUOUS_STOCK'); quit(1); }
    const r = db.stock.updateOne({ _id: matches[0]._id }, { $set: { quantity: ${quantity} } });
    print(r.matchedCount === 1 ? 'OK' : 'NOMATCH');
  `;
  let output: string;
  try {
    output = exec('docker', ['exec', container,
      ...mongoShell(lease), '--quiet', '--eval', script],
    { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  } catch (error) {
    if (/^AMBIGUOUS_(PARENT|STOCK)$/m.test(streams(error, 'stdout').trim())) {
      throw new Error('MongoDB stock correction refused: multiple matching item, warehouse, or stock rows', { cause: error });
    }
    const missingRow = /^MISSING_(ITEM|WAREHOUSE)$/m.exec(streams(error, 'stdout').trim())?.[1];
    if (missingRow) throw stockInterfaceError(`required ${missingRow.toLowerCase()} row is absent`,
      { cause: error, missingRow: missingRow === 'ITEM' ? 'item' : 'warehouse' });
    if (!/^MISSING$/m.test(streams(error, 'stdout').trim())) throw error;
    const detail = streams(error, 'stdout', 'stderr').trim().slice(-160);
    throw stockInterfaceError('direct stock correction requires singular collections '
      + '`item`, `warehouse`, and `stock`; stock rows must use '
      + `item_id/warehouse_id or itemId/warehouseId: ${detail}`,
    { cause: error });
  }
  if (/^NOMATCH$/m.test(output.trim())) {
    throw stockInterfaceError(`could not find ${item} / ${warehouse} in the required collections `
      + `(${output.trim().slice(0, 80)})`, { missingRow: 'stock' });
  }
  if (!/^OK$/m.test(output.trim())) throw new Error(`unexpected MongoDB stock write result: ${output.trim().slice(-160)}`);
  return { backend: 'mongodb', item, warehouse, quantity };
}

export function getMongoDbStock({ item, warehouse, lease, exec = execFileSync }: {
  item: string; warehouse?: string; lease: LeasedDatabase; exec?: TextCommandExecutor;
}): { backend: string; item: string; warehouse?: string; quantity: number } {
  const container = assertLeasedContainer(lease.resources.container, exec, WRITE_TIMEOUT_MS,
    'direct database read');
  const script = `
    function reject(error, missingRow, invalid = false) { print(JSON.stringify({ error, missingRow, invalid })); quit(0); }
    const collections = db.getCollectionNames();
    if (!['item', 'warehouse', 'stock'].every(name => collections.includes(name))) reject('required singular stock collections are absent');
    const items = db.item.find({ name: ${JSON.stringify(item)} }).limit(2).toArray();
    if (!items.length) reject('required item is missing', 'item');
    if (items.length !== 1) reject('required item is ambiguous', undefined, true);
    const warehouses = db.warehouse.find(${warehouse === undefined ? '{}' : `{ name: ${JSON.stringify(warehouse)} }`}).toArray();
    if (!warehouses.length) reject('required warehouse is missing', 'warehouse');
    ${warehouse === undefined ? '' : "if (warehouses.length !== 1) reject('required warehouse is ambiguous', undefined, true);"}
    function references(id) {
      if (id && id._bsontype === 'ObjectId') return [id, id.toHexString()];
      if (typeof id === 'string' && /^[0-9a-f]{24}$/.test(id)) return [id, new ObjectId(id)];
      return [id];
    }
    function key(id) { return id && id._bsontype === 'ObjectId' ? id.toHexString() : String(id); }
    const iid = references(items[0].id ?? items[0]._id);
    const wid = ${warehouse === undefined ? 'null' : 'references(warehouses[0].id ?? warehouses[0]._id)'};
    const matches = db.stock.find({ $or: [
      { item_id: { $in: iid }${warehouse === undefined ? '' : ', warehouse_id: { $in: wid }'} },
      { itemId: { $in: iid }${warehouse === undefined ? '' : ', warehouseId: { $in: wid }'} }
    ] }).toArray();
    if (!matches.length) reject('required stock row is absent', 'stock');
    const seen = new Set();
    for (const row of matches) {
      const id = key(row.warehouse_id ?? row.warehouseId);
      if (warehouses.filter(w => key(w.id ?? w._id) === id).length !== 1 || seen.has(id)) reject('stock warehouse links are invalid or duplicated', undefined, true);
      seen.add(id);
    }
    print(JSON.stringify({ quantities: matches.map(row => row.quantity) }));
  `;
  const output = exec('docker', ['exec', container, ...mongoShell(lease), '--quiet', '--eval', script],
    { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS }).trim();
  let result: unknown;
  try { result = JSON.parse(output); } catch {
    throw new Error('MongoDB stock read returned invalid JSON');
  }
  if (!record(result)) throw new Error('MongoDB stock read returned an invalid result');
  if (typeof result.error === 'string') {
    throw stockInterfaceError(result.error, {
      invalid: result.invalid === true,
      ...(result.missingRow === 'item' || result.missingRow === 'warehouse' || result.missingRow === 'stock'
        ? { missingRow: result.missingRow } : {}),
    });
  }
  if (!Array.isArray(result.quantities) || !result.quantities.length) {
    throw stockInterfaceError('stock read returned no quantities', { missingRow: 'stock' });
  }
  const quantity = stockQuantity(result.quantities.reduce((sum: number, value: unknown) =>
    stockQuantity(sum + stockQuantity(value)), 0));
  return { backend: 'mongodb', item, ...(warehouse === undefined ? {} : { warehouse }), quantity };
}

export function prepareMongoDbDatabase({ lease, name, expectedName, wipe,
  exec = execFileSync }: {
  lease: LeasedDatabase; name: string; expectedName: string; wipe: boolean;
  exec?: TextCommandExecutor;
}): string {
  if (name !== expectedName || name !== lease.resources.database) {
    throw new Error(`backend lease database ${name} does not match harness target ${expectedName}`);
  }
  if (wipe) {
    try {
      resetMongoDb({ lease, exec });
      console.error(`  wiped ${name} — a build starts on an empty database`);
    } catch (error) {
      throw new Error(`could not wipe ${name}: ${streams(error, 'message').split('\n')[0]}`,
      { cause: error });
    }
  } else assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS, 'database mutation');
  return name;
}
