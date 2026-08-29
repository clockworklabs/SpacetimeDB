import { execFileSync } from 'node:child_process';

import { assertLeasedContainer } from '../backend-reset-guard.js';
import { databaseContainerName } from '../database-containers.js';

const RESET_TIMEOUT_MS = 120_000;
const WRITE_TIMEOUT_MS = 60_000;

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object';

// A failed child process carries its output on the error.
const streams = (error: unknown, ...keys: readonly string[]): string =>
  record(error) ? keys.map(key => String(error[key] ?? '')).join('') : '';

type Exec = typeof execFileSync;
type LeasedDatabase = {
  resources: { container: { id: string; name: string }; database: string };
};

export function resetMongoDb({ lease, exec = execFileSync }:
  { lease: LeasedDatabase; exec?: Exec }): string {
  assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS, 'reset');
  exec('docker', ['exec', lease.resources.container.name, 'mongosh', lease.resources.database,
    '--quiet', '--eval', 'db.dropDatabase()'], { stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
  return `reset mongodb database ${lease.resources.database}`;
}

export function proveMongoDbUse({ lease, marker, exec = execFileSync }:
  { lease: LeasedDatabase; marker: unknown; exec?: Exec }):
  { ok: boolean; verified: boolean; matches: number; reason: string } {
  if (typeof marker !== 'string' || !marker) {
    throw new Error('MongoDB provenance requires a non-empty application marker');
  }
  assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS,
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
  const output = exec('docker', ['exec', lease.resources.container.name,
    'mongosh', lease.resources.database, '--quiet', '--eval', script],
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

export function setMongoDbStock({ item, warehouse, quantity, dbName, exec = execFileSync,
  containers = {} }: {
  item: string; warehouse: string; quantity: number; dbName: string;
  exec?: Exec; containers?: { mongodb?: string };
}): { backend: string; item: string; warehouse: string; quantity: number } {
  const container = containers.mongodb ?? databaseContainerName('mongodb');
  const script = `
    const it = db.item.findOne({ name: ${JSON.stringify(item)} });
    const wh = db.warehouse.findOne({ name: ${JSON.stringify(warehouse)} });
    if (!it || !wh) { print('MISSING'); quit(1); }
    const iid = it._id ?? it.id, wid = wh._id ?? wh.id;
    const r = db.stock.updateOne(
      { $or: [ { item_id: iid, warehouse_id: wid }, { itemId: iid, warehouseId: wid } ] },
      { $set: { quantity: ${quantity} } });
    print(r.matchedCount === 1 ? 'OK' : 'NOMATCH');
  `;
  let output: string;
  try {
    output = exec('docker', ['exec', container,
      'mongosh', dbName, '--quiet', '--eval', script],
    { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  } catch (error) {
    const detail = streams(error, 'stdout', 'stderr').trim().slice(-160);
    throw new Error('direct stock correction requires singular collections '
      + '`item`, `warehouse`, and `stock`; stock rows must use '
      + `item_id/warehouse_id or itemId/warehouseId: ${detail || streams(error, 'message')}`,
    { cause: error });
  }
  if (!/OK/.test(output)) {
    throw new Error(`could not find ${item} / ${warehouse} in the required collections `
      + `(${output.trim().slice(0, 80)})`);
  }
  return { backend: 'mongodb', item, warehouse, quantity };
}

export function prepareMongoDbDatabase({ lease, name, expectedName, wipe,
  exec = execFileSync }: {
  lease: LeasedDatabase; name: string; expectedName: string; wipe: boolean; exec?: Exec;
}): string {
  if (name !== expectedName) {
    throw new Error(`backend lease database ${name} does not match harness target ${expectedName}`);
  }
  assertLeasedContainer(lease.resources.container, exec, RESET_TIMEOUT_MS, 'database mutation');
  if (wipe) {
    try {
      exec('docker', ['exec', lease.resources.container.name, 'mongosh', name, '--quiet',
        '--eval', 'db.dropDatabase()'], { stdio: 'pipe', timeout: RESET_TIMEOUT_MS });
      console.error(`  wiped ${name} — a build starts on an empty database`);
    } catch (error) {
      throw new Error(`could not wipe ${name}: ${streams(error, 'message').split('\n')[0]}`,
      { cause: error });
    }
  }
  return name;
}
