import { execFileSync } from 'node:child_process';
import { stockInterfaceError } from '../stock-interface.js';

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
    const it = db.item.findOne({ name: ${JSON.stringify(item)} });
    const wh = db.warehouse.findOne({ name: ${JSON.stringify(warehouse)} });
    if (!it || !wh) { print('MISSING'); quit(1); }
    const iid = it.id ?? it._id, wid = wh.id ?? wh._id;
    const r = db.stock.updateOne(
      { $or: [ { item_id: iid, warehouse_id: wid }, { itemId: iid, warehouseId: wid } ] },
      { $set: { quantity: ${quantity} } });
    print(r.matchedCount === 1 ? 'OK' : 'NOMATCH');
  `;
  let output: string;
  try {
    output = exec('docker', ['exec', container,
      ...mongoShell(lease), '--quiet', '--eval', script],
    { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  } catch (error) {
    if (!/^MISSING$/m.test(streams(error, 'stdout').trim())) throw error;
    const detail = streams(error, 'stdout', 'stderr').trim().slice(-160);
    throw stockInterfaceError('direct stock correction requires singular collections '
      + '`item`, `warehouse`, and `stock`; stock rows must use '
      + `item_id/warehouse_id or itemId/warehouseId: ${detail}`,
    { cause: error });
  }
  if (/^NOMATCH$/m.test(output.trim())) {
    throw stockInterfaceError(`could not find ${item} / ${warehouse} in the required collections `
      + `(${output.trim().slice(0, 80)})`);
  }
  if (!/^OK$/m.test(output.trim())) throw new Error(`unexpected MongoDB stock write result: ${output.trim().slice(-160)}`);
  return { backend: 'mongodb', item, warehouse, quantity };
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
