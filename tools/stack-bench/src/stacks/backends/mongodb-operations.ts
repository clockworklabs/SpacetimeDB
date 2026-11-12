import { execFileSync, spawn } from 'node:child_process';
import { stockInterfaceError, stockQuantity } from '../stock-interface.js';
import { checkoutId, checkoutMinor, checkoutStateSchema, verifyCheckoutSchema } from '../checkout-state.js';
import { orderDataColumns, readOrderDataSnapshot, type OrderDataStorage } from '../order-data.js';

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

const orderDataReadScript = `
  function readOrderData(columns) {
    const session=db.getMongo().startSession();
    const store=session.getDatabase(db.getName());
    try {
      const names=db.getCollectionNames(), result={};
      session.startTransaction({readConcern:{level:'snapshot'}});
      for (const [table, fields] of Object.entries(columns)) {
        // MongoDB creates a collection on its first insert; until then it has no rows.
        if (!names.includes(table)) { result[table]=[]; continue; }
        result[table]=store.getCollection(table).find({}).toArray().map(row=>Object.fromEntries(fields.map(field=>{
          let value=field==='id' ? row.id ?? row._id : row[field];
          if (value && value._bsontype) {
            value=value.toString();
            if (field==='quantity') value=Number(value);
          }
          return [field,value];
        })));
      }
      session.commitTransaction();
      return result;
    } finally { session.endSession(); }
  }
`;

export function createMongoDbOrderDataReader({ lease, exec = execFileSync }: {
  lease: LeasedDatabase; exec?: TextCommandExecutor;
}) {
  assertLeasedContainer(lease.resources.container, exec, WRITE_TIMEOUT_MS, 'order data reader start');
  const script = `${orderDataReadScript}
    const input=require('readline').createInterface({input:process.stdin});
    input.on('line', line=>{
      let id;
      try {
        const request=JSON.parse(line);
        id=request.id;
        print(JSON.stringify({id,result:readOrderData(request.columns)}));
      } catch(error) { print(JSON.stringify({id,error:String(error)})); }
    });
    new Promise(resolve=>input.once('close',resolve));`;
  const child = spawn('docker', ['exec', '-i', lease.resources.container.id,
    ...mongoShell(lease), '--quiet', '--eval', script],
  { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
  let pending: { id: number; resolve(value: unknown): void; reject(error: Error): void } | null = null;
  let nextId = 0, closed = false;
  let closeTask: Promise<void> | undefined;
  let output = Buffer.alloc(0);
  const exited = new Promise<void>(resolve => child.once('close', () => resolve()));
  const fail = (error: Error) => { pending?.reject(error); pending = null; };
  const broken = (message: string) => {
    closed = true;
    fail(new Error(message));
    void close().catch(() => {});
  };
  const onLine = (line: string) => {
    if (!pending) { broken('MongoDB order reader returned an unsolicited reply'); return; }
    let reply: unknown;
    try { reply = JSON.parse(line); }
    catch { broken('MongoDB order reader returned invalid JSON'); return; }
    if (!record(reply) || reply.id !== pending.id || !('result' in reply || 'error' in reply)) {
      broken('MongoDB order reader returned an invalid reply'); return;
    }
    const current = pending;
    pending = null;
    if (typeof reply.error === 'string') current.reject(new Error(reply.error));
    else if ('result' in reply) current.resolve(reply.result);
    else current.reject(new Error('MongoDB order reader returned an invalid reply'));
  };
  // execFileSync's prior stdout limit was 1 MiB. Reject a larger or unterminated reply.
  child.stdout.on('data', (chunk: Buffer) => {
    if (closed) return;
    output = Buffer.concat([output, chunk]);
    if (output.length > 1024 * 1024) { broken('MongoDB order reader reply is too large'); return; }
    for (let end: number; (end = output.indexOf(10)) !== -1;) {
      const line = output.subarray(0, end).toString('utf8').trimEnd();
      output = Buffer.from(output.subarray(end + 1));
      onLine(line);
      if (closed) return;
    }
  });
  child.stderr.resume();
  child.on('error', () => broken('MongoDB order reader process failed'));
  child.on('close', () => broken(output.length
    ? 'MongoDB order reader returned a partial reply' : 'MongoDB order reader process closed'));
  child.stdin.on('error', () => broken('MongoDB order reader input failed'));
  let serial: Promise<unknown> = Promise.resolve();
  const waitForExit = async () => {
    let timer: ReturnType<typeof setTimeout>;
    try {
      return await Promise.race([exited.then(() => true),
        new Promise<boolean>(resolve => { timer = setTimeout(() => resolve(false), 2_000); })]);
    } finally { clearTimeout(timer!); }
  };
  const close = () => closeTask ??= (async () => {
    closed = true;
    fail(new Error('MongoDB order reader closed'));
    child.stdin.end();
    if (!await waitForExit()) {
      child.kill();
      await waitForExit();
      throw new Error('MongoDB order reader needed a forced stop; remote cleanup is unconfirmed');
    }
    if (child.exitCode !== 0) throw new Error('MongoDB order reader exited without clean cleanup');
  })();
  const read = ({ account, item, storage, signal }: {
    account: string; item: string; storage: OrderDataStorage; signal?: AbortSignal;
  }) => {
    const task = serial.then(async () => {
      try {
        if (closed) throw new Error('MongoDB order reader is closed');
        signal?.throwIfAborted();
        assertLeasedContainer(lease.resources.container, exec, WRITE_TIMEOUT_MS, 'order data read');
        signal?.throwIfAborted();
        const id = ++nextId;
        const raw = await new Promise<unknown>((resolve, reject) => {
          pending = { id, resolve, reject };
          const timer = setTimeout(() => pending?.id === id && broken('MongoDB order reader timed out'), WRITE_TIMEOUT_MS);
          const abort = () => broken('MongoDB order reader was cancelled');
          signal?.addEventListener('abort', abort, { once: true });
          const settle = (value: unknown, error?: Error) => {
            clearTimeout(timer);
            signal?.removeEventListener('abort', abort);
            if (error) reject(error); else resolve(value);
          };
          pending.resolve = value => settle(value);
          pending.reject = error => settle(undefined, error);
          child.stdin.write(`${JSON.stringify({ id, columns: orderDataColumns(storage) })}\n`);
        });
        return readOrderDataSnapshot(raw, account, item, storage);
      } catch (error) {
        await close();
        throw error;
      }
    });
    serial = task.catch(() => {});
    return task;
  };
  return { read, close };
}

export function getMongoDbCheckoutState({ account, item, app, lease, storage, exec = execFileSync }: {
  account: string; item: string; app: string; lease: LeasedDatabase; storage?: OrderDataStorage; exec?: TextCommandExecutor;
}) {
  if (storage?.kind === 'order-data') {
    const container = assertLeasedContainer(lease.resources.container, exec, WRITE_TIMEOUT_MS, 'order data read');
    const script = `
      ${orderDataReadScript}
      const columns=${JSON.stringify(orderDataColumns(storage))};
      print(JSON.stringify(readOrderData(columns)));
    `;
    const output = exec('docker', ['exec', container, ...mongoShell(lease), '--quiet', '--eval', script],
      { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
    return readOrderDataSnapshot(JSON.parse(output.trim()), account, item, storage);
  }
  const schemaSha256 = verifyCheckoutSchema('mongodb', app, ['server/src/models.ts', 'server/src/progression-models.ts']);
  const container = assertLeasedContainer(lease.resources.container, exec, WRITE_TIMEOUT_MS, 'checkout state read');
  const script = `
    const minor = ${checkoutMinor.toString()};
    const exactId = ${checkoutId.toString()};
    const key = value => exactId(value && typeof value.toHexString === 'function' ? value.toHexString() : value);
    const session = db.getMongo().startSession();
    const store = session.getDatabase(db.getName());
    try {
      session.startTransaction({readConcern:{level:'snapshot'}});
      for (const name of ['users','carts','orders','progressionpayments','item','stock']) {
        if (!db.getCollectionNames().includes(name)) throw new Error('checkout collection missing: '+name);
      }
      const users = store.users.find({username:${JSON.stringify(account)}}).toArray();
      const items = store.item.find({name:${JSON.stringify(item)}}).toArray();
      if (users.length!==1 || items.length!==1) throw new Error('checkout account or item is missing or ambiguous');
      const user=users[0], item=items[0];
      const carts=store.carts.find({userId:user._id}).toArray();
      if (carts.length>1) throw new Error('checkout cart mapping is ambiguous');
      const lines=carts.flatMap(cart=>cart.items);
      const state={accountId:key(user._id),itemId:key(item._id),priceMinor:minor(item.price),
        cart:lines.map(line=>({itemId:key(line.itemId),quantity:line.quantity})),
        stock:store.stock.find({item_id:item._id}).toArray().map(row=>({warehouseId:key(row.warehouse_id),quantity:row.quantity})),
        reservations:lines.flatMap(line=>line.reservedWarehouseIds.map(warehouseId=>({itemId:key(line.itemId),warehouseId:key(warehouseId),quantity:1}))),
        orders:store.orders.find({}).toArray().map(order=>({id:key(order._id),accountId:key(order.userId),
          totalMinor:minor(order.total),status:order.status,lines:order.items.map(line=>({itemId:key(line.itemId),quantity:line.quantity,priceMinor:minor(line.price),
            allocations:line.allocations.map(row=>({warehouseId:key(row.warehouseId),quantity:row.quantity}))}))})),
        payments:store.progressionpayments.find({}).toArray().map(row=>({id:key(row._id),orderId:key(row.orderId),amountMinor:minor(row.amount),status:row.status})),
        orphanOrderLines:0};
      session.commitTransaction();
      print(JSON.stringify(state));
    } finally { session.endSession(); }
  `;
  const output = exec('docker', ['exec', container, ...mongoShell(lease), '--quiet', '--eval', script],
    { encoding: 'utf8', stdio: 'pipe', timeout: WRITE_TIMEOUT_MS });
  return { schemaSha256, state: checkoutStateSchema.parse(JSON.parse(output.trim())) };
}

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
  if (name.startsWith('system.')) continue;
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
    const matches = db.stock.find({ item_id: { $in: iid }, warehouse_id: { $in: wid } }).limit(2).toArray();
    if (!matches.length) { print('NOMATCH'); quit(0); }
    if (matches.length !== 1) { print('AMBIGUOUS_STOCK'); quit(1); }
    if (!Number.isSafeInteger(matches[0].quantity)) { print('INVALID_QUANTITY'); quit(1); }
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
      throw stockInterfaceError('stock write is ambiguous: multiple matching item, warehouse, or stock rows',
        { cause: error, invalid: true });
    }
    if (/^INVALID_QUANTITY$/m.test(streams(error, 'stdout').trim())) {
      throw stockInterfaceError('stock quantity must be a safe whole number', { cause: error, invalid: true });
    }
    const missingRow = /^MISSING_(ITEM|WAREHOUSE)$/m.exec(streams(error, 'stdout').trim())?.[1];
    if (missingRow) throw stockInterfaceError(`required ${missingRow.toLowerCase()} row is absent`,
      { cause: error, missingRow: missingRow === 'ITEM' ? 'item' : 'warehouse' });
    if (!/^MISSING$/m.test(streams(error, 'stdout').trim())) throw error;
    const detail = streams(error, 'stdout', 'stderr').trim().slice(-160);
    throw stockInterfaceError('direct stock correction requires singular collections '
      + '`item`, `warehouse`, and `stock`; stock rows must use '
      + `item_id/warehouse_id: ${detail}`,
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
    const matches = db.stock.find({ item_id: { $in: iid }${warehouse === undefined ? '' : ', warehouse_id: { $in: wid }'} }).toArray();
    if (!matches.length) reject('required stock row is absent', 'stock');
    const seen = new Set();
    for (const row of matches) {
      const id = key(row.warehouse_id);
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
