import { execFileSync } from 'node:child_process';
import { loadTrack, portsFor } from '../../composition/tracks.js';
import { z } from 'zod';
import { leaseFromEnv, loopbackHttpUri, type BackendLease } from '../../runtime/backend-lease.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';
import { assertLeasedContainer } from '../backend-reset-guard.js';
import { requireAttemptNetwork } from '../../runtime/docker-network.js';
import { orderDataColumns, orderDataError, readOrderDataSnapshot, type OrderDataStorage } from '../order-data.js';
import { stockInterfaceError, stockQuantity } from '../stock-interface.js';
import type { NamedAction } from '../../composition/tracks.js';
import { convexFunctionRequest, classifyConvexFunctionResponse } from './convex-protocol.js';

const TIMEOUT = 30_000;
const record = z.record(z.string(), z.unknown());
interface NativeInput { lease?: BackendLease; exec?: TextCommandExecutor }

function target(input: NativeInput) {
  const lease = input.lease ?? leaseFromEnv(process.env, { backend: 'convex', active: true }).lease;
  if (lease.backend !== 'convex' || !lease.resources.serverUri || !lease.resources.container) {
    throw new Error('Convex operation requires its authenticated native backend lease');
  }
  const exec: TextCommandExecutor = input.exec ?? execFileSync;
  requireAttemptNetwork(lease, exec);
  const container = assertLeasedContainer(lease.resources.container, exec, TIMEOUT, 'Convex native operation');
  return { lease, exec, container, uri: loopbackHttpUri(lease.resources.serverUri).origin };
}

export function convexAdminKey(lease: BackendLease, exec: TextCommandExecutor = execFileSync): string {
  return ownedAdminKey(target({ lease, exec }));
}

function ownedAdminKey(owned: ReturnType<typeof target>): string {
  const key = owned.exec('docker', ['exec', owned.container, 'bash', './generate_admin_key.sh'],
    { encoding: 'utf8', stdio: 'pipe', timeout: TIMEOUT }).trim();
  if (!key || /[\r\n]/.test(key)) throw new Error('Convex admin credential is unavailable');
  return key;
}

export function convexApplicationEnvironment(lease: BackendLease): Record<string, string> {
  if (!lease.resources.serverUri) throw new Error('Convex application requires a leased native URL');
  const sitePort = portsFor(loadTrack(lease.track), 'convex', lease.runIndex).express;
  if (!sitePort) throw new Error('Convex application requires a leased HTTP action port');
  return { CONVEX_SITE_URL: `http://127.0.0.1:${sitePort}`, CONVEX_SELF_HOSTED_URL: lease.resources.serverUri,
    CONVEX_SELF_HOSTED_ADMIN_KEY: convexAdminKey(lease), VITE_CONVEX_URL: lease.resources.serverUri };
}

// Vendor native administrative APIs, pinned with the backend image. This never
// invokes an application's observer or business function. Credentials use stdin.
function admin(input: NativeInput) {
  const deadline = Date.now() + TIMEOUT;
  const owned = target(input);
  const key = ownedAdminKey(owned);
  const post = (endpoint: string, body: unknown): string => {
    const remaining = deadline - Date.now();
    if (remaining <= 0) throw new Error('Convex native observation exceeded its time budget');
    const config = [`url = ${JSON.stringify(owned.uri + endpoint)}`,
      `header = ${JSON.stringify('Authorization: Convex ' + key)}`,
      'header = "Content-Type: application/json"', `data = ${JSON.stringify(JSON.stringify(body))}`].join('\n');
    try {
      // Activation publishes this owned backend's port for the trusted controller.
      // Avoid a Docker exec per request. Ignore local curl config and proxies so
      // the admin credential only goes to the leased loopback endpoint.
      return owned.exec('curl', ['--disable', '--noproxy', '*', '--silent', '--show-error',
        '--fail', '--max-time', String(remaining / 1000), '--config', '-'],
      { encoding: 'utf8', stdio: 'pipe', timeout: remaining, input: config });
    } catch { throw new Error('Convex native administrative request failed'); }
  };
  const call = (kind: 'query' | 'mutation', path: string, args: Record<string, unknown>) => {
    const result = classifyConvexFunctionResponse(200, post('/api/' + kind, { path, args, format: 'json' }));
    if (result.kind !== 'accepted') throw new Error('Convex native administrative function failed');
    return result.value;
  };
  return { post, call };
}

function tableNames(client: ReturnType<typeof admin>): string[] {
  const result = z.object({ page: z.array(z.object({ name: z.string() })), isDone: z.literal(true) })
    .parse(client.call('query', '_system/cli/tables', { paginationOpts: { cursor: null, numItems: 1000 } }));
  return result.page.map(row => row.name);
}

// The native sync API brings all selected tables to one snapshot. Per-table
// queries made at different times cannot establish conservation invariants.
export function readConvexTables(names: readonly string[], input: NativeInput = {}) {
  const client = admin(input);
  const available = tableNames(client);
  if (names.some(name => !available.includes(name))) throw orderDataError('required Convex data table is missing');
  const selection = { _other: 'excluded', '': { _other: 'excluded',
    ...Object.fromEntries(names.map(name => [name, { _other: 'included' }])) } };
  const tables = new Map<string, Map<string, Record<string, unknown>>>();
  let cursor: string | undefined;
  const pageSchema = z.object({
    truncates: z.array(z.object({ component: z.string(), table: z.string() })),
    values: z.array(z.object({ component: z.string(), table: z.string(), deleted: z.boolean(), value: record })),
    status: z.object({ type: z.enum(['upToDate', 'snapshotting', 'stale']), snapshotTs: z.number().finite().optional() }),
    pagination: z.object({ nextCursor: z.string().nullable().optional() }),
  });
  for (let page = 0; page < 100; page++) {
    const raw = client.post('/api/v1/data/sync', { selection, ...(cursor ? { cursor } : {}) });
    const result = pageSchema.parse(JSON.parse(raw));
    for (const row of result.truncates) {
      if (row.component !== '' || !names.includes(row.table)) throw new Error('Convex snapshot returned an unselected table');
      tables.set(row.table, new Map());
    }
    for (const row of result.values) {
      if (row.component !== '' || !tables.has(row.table) || typeof row.value._id !== 'string') {
        throw new Error('Convex snapshot returned an invalid document');
      }
      if (row.deleted) tables.get(row.table)!.delete(row.value._id);
      else tables.get(row.table)!.set(row.value._id, row.value);
    }
    if (result.status.type === 'upToDate') {
      if (names.some(name => !tables.has(name))) throw new Error('Convex snapshot omitted a selected table');
      if (result.status.snapshotTs === undefined) throw new Error('Convex snapshot omitted its consistency timestamp');
      return Object.fromEntries([...tables].map(([name, rows]) => [name, [...rows.values()]]));
    }
    const next = result.pagination.nextCursor;
    if (!next || next === cursor) throw new Error('Convex snapshot cursor did not advance');
    cursor = next;
  }
  throw new Error('Convex snapshot did not reach a consistent boundary within 100 pages');
}

export function getConvexCheckoutState({ account, item, storage, ...input }: NativeInput & {
  account: string; item: string; app?: string; storage?: OrderDataStorage;
}) {
  if (!storage) throw orderDataError('Convex requires the declared order data interface');
  const columns = orderDataColumns(storage);
  const tables = readConvexTables(Object.keys(columns), input);
  const rows = Object.fromEntries(Object.entries(columns).map(([table, fields]) => [table,
    tables[table]!.map(row => Object.fromEntries(fields.map(field => [field, field === 'id' ? row._id : row[field]])))]));
  return readOrderDataSnapshot(rows, account, item, storage);
}

function stockRows(item: string, warehouse: string | undefined, input: NativeInput) {
  let tables: ReturnType<typeof readConvexTables>;
  try { tables = readConvexTables(['item', 'warehouse', 'stock'], input); }
  catch (error) {
    if ((error as { orderDataInterface?: boolean }).orderDataInterface) throw stockInterfaceError('required Convex stock tables are missing');
    throw error;
  }
  const items = tables.item!.filter(row => row.name === item);
  const warehouses = tables.warehouse!.filter(row => warehouse === undefined || row.name === warehouse);
  if (!items.length) throw stockInterfaceError('required item is absent', { missingRow: 'item' });
  if (!warehouses.length) throw stockInterfaceError('required warehouse is absent', { missingRow: 'warehouse' });
  if (items.length !== 1 || (warehouse !== undefined && warehouses.length !== 1)) {
    throw stockInterfaceError('stock parents are ambiguous', { invalid: true });
  }
  const rows = tables.stock!.filter(row => row.item_id === items[0]!._id
    && (warehouse === undefined || row.warehouse_id === warehouses[0]!._id));
  if (!rows.length) throw stockInterfaceError('required stock row is absent', { missingRow: 'stock' });
  const seen = new Set();
  for (const row of rows) {
    if (seen.has(row.warehouse_id) || warehouses.filter(parent => parent._id === row.warehouse_id).length !== 1) {
      throw stockInterfaceError('stock warehouse links are invalid or duplicated', { invalid: true });
    }
    seen.add(row.warehouse_id);
    stockQuantity(row.quantity);
  }
  return rows;
}

export function getConvexStock({ item, warehouse, ...input }: NativeInput & { item: string; warehouse?: string }) {
  const quantity = stockRows(item, warehouse, input).reduce((sum, row) => stockQuantity(sum + stockQuantity(row.quantity)), 0);
  return { backend: 'convex', item, ...(warehouse === undefined ? {} : { warehouse }), quantity };
}

export function setConvexStock({ item, warehouse, quantity, ...input }: NativeInput & {
  item: string; warehouse: string; quantity: number;
}) {
  stockQuantity(quantity);
  const rows = stockRows(item, warehouse, input);
  const result = admin(input).call('mutation', '_system/frontend/patchDocumentsFields', {
    table: 'stock', ids: [rows[0]!._id], fields: { quantity }, componentId: null,
  });
  z.object({ success: z.literal(true) }).parse(result);
  return { backend: 'convex', item, warehouse, quantity };
}

export function proveConvexUse({ marker, ...input }: NativeInput & { marker: unknown }) {
  if (typeof marker !== 'string' || !marker) throw new Error('Convex provenance requires an application marker');
  const names = tableNames(admin(input));
  const tables = readConvexTables(names, input);
  const contains = (value: unknown): boolean => value === marker || (value !== null && typeof value === 'object'
    && Object.values(value).some(contains));
  const matches = Object.values(tables).filter(rows => rows.some(contains)).length;
  return { ok: matches > 0, verified: true, matches,
    reason: matches ? 'application marker exists in the leased Convex backend' : 'application marker is absent from the leased Convex backend' };
}

export function convexNamedActionRequest({ action, input, url }: { action: NamedAction; input?: unknown; spacetime?: unknown; url?: string | null }) {
  if (!action.reducer) return null;
  const supplied = (input && typeof input === 'object' ? input : {}) as {
    values?: Record<string, unknown>; args?: readonly unknown[]; body?: Record<string, unknown>;
  };
  const params = action.params ?? [];
  const args = supplied.args ?? action.args ?? [];
  if (!supplied.values && args.length && params.length !== args.length) {
    throw Object.assign(new Error('Convex named action requires declared argument names'), { code: 'invalid_named_action_input' });
  }
  const values = supplied.values ?? supplied.body ?? Object.fromEntries(params.map((param, index) => [param.name, args[index]]));
  const lease = leaseFromEnv(process.env, { backend: 'convex', active: true }).lease;
  const request = convexFunctionRequest({ deploymentUrl: lease.resources.serverUri!, kind: 'mutation',
    path: `api:${action.reducer}`, args: values });
  return { ...request, responseContract: 'convex-mutation' as const,
    ...(url ? { applicationOrigin: new URL(url).origin } : {}) };
}

// Same native metadata owner as the vendor `convex function-spec` command.
// Presence is a diagnostic only; this does not establish a successful effect.
export function probeConvexNamedAction(action: NamedAction, input: NativeInput = {}) {
  const functions = z.array(record).parse(admin(input).call('query', '_system/cli/modules:apiSpec', {}));
  const found = functions.filter(fn => fn.identifier === `api.js:${action.reducer}`
    && fn.functionType === 'Mutation' && (fn.visibility as { kind?: unknown } | undefined)?.kind === 'public');
  return { ok: found.length === 1, status: 0,
    note: found.length === 1 ? 'declared public native mutation exists; behavior is checked separately'
      : `missing public native mutation api:${action.reducer}` };
}
