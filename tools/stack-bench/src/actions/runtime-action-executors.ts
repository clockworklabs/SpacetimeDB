import { execFileSync } from 'node:child_process';
import { evidenceNowMs } from '../evidence/evidence-timing.js';
import { resolveOrderDataStorage, type OrderDataSelection, type OrderDataStorage } from '../stacks/order-data.js';

import { ActionApplicationFailure, ActionInconclusive, actionImplementation } from './action-contract.js';
import { finding, isFinding, renderFinding } from './action-findings.js';
import { checkoutDifferences, orderCheckoutDifferences, orderCheckoutWithAddDifferences, orderOperationDifferences, type OrderOperation, cancellationDifferences, orderCancellationDifferences,
  purchaseDifferences, orderPurchaseDifferences, checkoutId } from '../stacks/checkout-state.js';
import { getSavedPostgresCheckoutState } from '../stacks/backends/saved-postgres-checkout.js';
import { getSavedMongoDbCheckoutState } from '../stacks/backends/saved-mongodb-checkout.js';
import { getSavedSpacetimeCheckoutState } from '../stacks/backends/saved-spacetime-checkout.js';
import type { NamedActionsCapability } from './named-action-runtime.js';
import type { CheckoutState } from '../stacks/checkout-state.js';
import type {
  ActionImplementation,
} from './action-contract.js';
import { actorFor, fail, inconclusive } from './actor-action-runtime.js';
import type { NetworkInterruption } from './network-interruption.js';
import type { ActionCall } from './actor-action-runtime.js';
import { evidenceDisposition } from '../evidence/check-evidence.js';
import { redactCredentials } from '../evidence/diagnostic-sanitizer.js';
import type { CheckEvidenceStatus } from '../evidence/check-evidence.js';
import { replayHeaders } from './actor-transport-action-executors.js';
import { browserApplicationBoundary, numberMatches } from './browser-action-executors.js';
import { harnessProcessFailure } from '../evidence/harness-errors.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import type { LeasedDatabase } from '../stacks/backend-reset-guard.js';
import type { LeasedSpacetimeTarget } from '../runtime/spacetime-target.js';
import type { RuntimeControlMode, RuntimeControlSpec } from '../runtime/backend-control.js';
import type { TextCommandExecutor } from '../runtime/command-executor.js';

type UnknownRecord = Record<string, unknown>;
type Sleep = (milliseconds: number, signal: AbortSignal) => Promise<void>;
type Exec = TextCommandExecutor;

declare const navigator: { readonly onLine: boolean };

interface CapturedWrite {
  readonly body?: unknown;
  readonly headers: Readonly<Record<string, string>>;
  readonly method: string;
  readonly url: string;
}

interface Locator {
  click(options?: unknown): Promise<void>;
  isEnabled(): Promise<boolean>;
  waitFor(options?: unknown): Promise<void>;
}

interface Actor {
  readonly actionCall?: ActionCall;
  readonly lastWrite?: CapturedWrite | null;
  readonly lastWrites?: Readonly<Record<string, CapturedWrite | undefined>>;
  readonly page: {
    readonly request: {
      fetch(url: string, options: UnknownRecord): Promise<{ status(): number }>;
    };
    close(): Promise<void>;
    context(): { setOffline(offline: boolean): Promise<void> };
    evaluate<Result>(callback: () => Result): Promise<Result>;
  };
  readonly writes?: readonly CapturedWrite[];
  readonly networkInterruption?: NetworkInterruption;
  loc(testid: string, options?: unknown): Locator;
}

interface ActionStep extends UnknownRecord {
  readonly do: string;
}

interface ConcurrencyCapability {
  readonly defaultWithin: number;
  dispatch(step: ActionStep, signal: AbortSignal): Promise<unknown>;
  expand(value: string | undefined): string | undefined;
  sleep: Sleep;
  testId(id: string): string;
}

interface LifecycleCapability {
  operate(mode: 'restart' | 'start' | 'stop', settleMs: number, signal: AbortSignal): Promise<void>;
}

interface LifecycleConcurrencyCapabilities {
  readonly 'named-actions': NamedActionsCapability;
  readonly actors: { get(name: string): Actor | undefined };
  readonly 'application-lifecycle': LifecycleCapability;
  readonly 'backend-lifecycle': LifecycleCapability;
  readonly 'browser-interaction': {
    readonly clients: {
      fresh(actor: Actor, actorName: string, preserveStorage: boolean): Promise<string>;
      open(actor: Actor, settleMs: number, signal: AbortSignal): Promise<void>;
    };
    sleep: Sleep;
  };
  readonly clock: { sleep: Sleep };
  readonly concurrency: ConcurrencyCapability;
  readonly 'database-write': {
    setStock(input: SetStockInput): unknown | Promise<unknown>;
  };
  readonly 'database-read': {
    getStock(input: { item: string; warehouse?: string }):
      { quantity: number } | Promise<{ quantity: number }>;
    getCheckoutState(input: { account: string; item: string; storage?: OrderDataSelection }): CheckoutSnapshot;
    readonly checkoutSnapshots: Map<string, CheckoutSnapshot & { account: string; item: string }>;
  };
  readonly 'browser-observation': {
    readonly recorded: { get(key: string): number | undefined; set(key: string, value: number): void };
  };
}

interface CheckoutSnapshot {
  readonly state: CheckoutState;
  readonly catalog?: readonly { itemId: string; name: string; priceMinor: number }[];
  readonly schemaSha256: Record<string, string>;
  readonly recordedAtMs?: number;
  readonly scope?: 'orders';
  readonly storage?: OrderDataStorage;
}

interface ActionArguments<Input> {
  readonly input: Input;
  readonly capabilities: LifecycleConcurrencyCapabilities;
  readonly signal: AbortSignal;
}

interface ReplayConcurrentlyInput {
  readonly actors: readonly string[];
  readonly match?: string;
  readonly method?: string;
  readonly settleMs?: number;
}

interface LocatorScope {
  readonly contains?: string;
  readonly testid: string;
}

interface ClickTarget {
  readonly actor: string;
  readonly in?: LocatorScope;
}

interface ClickConcurrentlyInput {
  readonly actors: readonly string[];
  readonly in?: LocatorScope;
  readonly readyWithin?: number;
  readonly settleMs?: number;
  readonly targets?: readonly ClickTarget[];
  readonly testid: string;
  readonly within?: number;
}

interface RaceInput {
  readonly branches: readonly (readonly ActionStep[])[];
  readonly settleMs?: number;
}

interface ConcurrentSender {
  readonly actor: string;
  readonly count: number;
  readonly delayMs?: number;
  readonly prefix: string;
}

interface SendConcurrentlyInput {
  readonly delayMs?: number;
  readonly senders: readonly ConcurrentSender[];
}

interface SettleInput { readonly settleMs?: number }
interface ActorInput { readonly actor: string; readonly settleMs?: number }
interface OfflineInput extends ActorInput { readonly offline?: boolean }
interface SetStockInput {
  readonly item: string;
  readonly quantity: number;
  readonly settleMs: number;
  readonly warehouse: string;
}

interface ReadStockInput {
  readonly within?: number;
  readonly item: string;
  readonly warehouse?: string;
  readonly as?: string;
  readonly equals?: number;
  readonly atLeast?: number;
  readonly atMost?: number;
  readonly relativeTo?: string;
  readonly plus?: number;
}

async function dbRecordStock({ input, capabilities }: ActionArguments<ReadStockInput>) {
  const value = await capabilities['database-read'].getStock(input);
  capabilities['browser-observation'].recorded.set(input.as!, value.quantity);
  return { ...value, key: input.as };
}

async function dbRecordCheckout({ input, capabilities }: ActionArguments<{ account: string; item: string; as: string; storage?: OrderDataSelection }>) {
  const database = capabilities['database-read'];
  const snapshot = { ...database.getCheckoutState(input), recordedAtMs: evidenceNowMs() };
  if (snapshot.storage?.warehouses !== false && !snapshot.state.stock.length) inconclusive('invalid-input', { detail: 'checkout setup requires known stock warehouses' });
  database.checkoutSnapshots.set(input.as, { ...snapshot, account: input.account, item: input.item });
  return { ...snapshot, key: input.as };
}

async function dbExpectCatalogItem({ input, capabilities, signal }: ActionArguments<{
  before: string; name: string; priceMinor: number; within?: number;
}>) {
  const database = capabilities['database-read'];
  const before = database.checkoutSnapshots.get(input.before);
  if (!before) inconclusive('assertion-without-action', { action: 'dbRecordCheckout' });
  if (before.scope !== 'orders' || before.storage?.kind !== 'order-data' || !before.catalog) {
    throw new Error('catalog creation requires native catalog evidence');
  }
  if (before.catalog.some(row => row.name === input.name)) throw new Error('catalog creation name already exists in the baseline');
  const read = () => {
    const value = database.getCheckoutState(before);
    if (value.scope !== before.scope || !value.catalog
      || JSON.stringify(value.schemaSha256) !== JSON.stringify(before.schemaSha256)) throw new Error('catalog reader changed during creation');
    return value.catalog.filter(row => row.name === input.name);
  };
  const deadline = Date.now() + (input.within ?? 0);
  let matches = read();
  while (!matches.length && Date.now() < deadline) {
    await capabilities.clock.sleep(Math.min(250, deadline - Date.now()), signal);
    matches = read();
  }
  const observation = { before: input.before, name: input.name, matches, schemaSha256: before.schemaSha256 };
  const mismatch = matches.length !== 1
    ? { control: 'stored catalog entries for the created product', observed: matches.length, expected: { equals: 1 } }
    : matches[0]!.priceMinor !== input.priceMinor
      ? { control: 'stored product price in minor units', observed: matches[0]!.priceMinor, expected: { equals: input.priceMinor } } : null;
  if (mismatch) {
    const value = finding('number-mismatch', mismatch);
    throw new ActionApplicationFailure(renderFinding(value), { finding: value, observation });
  }
  return observation;
}

export type CheckoutQuantity = number | readonly { item: string; quantity: number }[];

export function checkoutExpectation(quantity: CheckoutQuantity, snapshots: readonly CheckoutSnapshot[]) {
  const before = snapshots[0]!;
  return typeof quantity === 'number' ? quantity : quantity.map(wanted => {
    if (before.scope !== 'orders') throw new Error('multi-item checkout requires native order data');
    const observed = snapshots.map(snapshot => {
      if (!snapshot.catalog) throw new Error('checkout reader did not provide catalog prices');
      const matches = snapshot.catalog.filter(row => row.name === wanted.item);
      if (matches.length !== 1) fail('interface-invalid', { action: 'read orders', attribute: 'item', detail: 'checkout item is missing or ambiguous' });
      return matches[0]!;
    });
    if (observed.some(row => row.itemId !== observed[0]!.itemId || row.priceMinor !== observed[0]!.priceMinor)) {
      fail('interface-invalid', { action: 'read orders', attribute: 'item', detail: 'checkout changed catalog identity or price' });
    }
    return { itemId: observed[0]!.itemId, priceMinor: observed[0]!.priceMinor, quantity: wanted.quantity };
  });
}

async function dbExpectCheckout({ input, capabilities }: ActionArguments<{ before: string; prepared: string;
  quantity: CheckoutQuantity; actor?: string; alongsideAdd?: string }>) {
  const database = capabilities['database-read'];
  const before = database.checkoutSnapshots.get(input.before);
  const prepared = database.checkoutSnapshots.get(input.prepared);
  if (!before || !prepared) inconclusive('assertion-without-action', { action: 'dbRecordCheckout' });
  if (before.account !== prepared.account || before.item !== prepared.item) throw new Error('checkout snapshots select different data');
  const after = database.getCheckoutState(before);
  if (JSON.stringify(before.schemaSha256) !== JSON.stringify(prepared.schemaSha256)
    || JSON.stringify(before.schemaSha256) !== JSON.stringify(after.schemaSha256)) throw new Error('checkout reader schema changed during the test');
  if (before.scope !== prepared.scope || before.scope !== after.scope) throw new Error('checkout scope changed');
  if (before.storage && !before.storage.cart) throw new Error('checkout reconciliation requires cart evidence');
  const quantity = checkoutExpectation(input.quantity, [before, prepared, after]);
  if (input.alongsideAdd && (before.scope !== 'orders' || typeof quantity === 'number' || input.actor)) {
    throw new Error('overlapping cart add requires native item-line expectations and separately verified accepted calls');
  }
  const response = input.actor ? actorFor(capabilities, input.actor).actionCall : undefined;
  if (input.actor && !response) inconclusive('assertion-without-action', { action: 'callAction' });
  if (response && (response.complete === false || !response.status)) inconclusive('transport-incomplete', {});
  if (response && response.action !== 'checkout') throw new Error('checkout reconciliation requires a checkout response');
  const refused = response !== undefined && !response.accepted;
  const compareState = refused ? prepared.state : after.state;
  const differences = input.alongsideAdd
    ? orderCheckoutWithAddDifferences(before.state, prepared.state, after.state,
      quantity as Exclude<typeof quantity, number>,
      (checkoutExpectation([{ item: input.alongsideAdd, quantity: 1 }], [before, prepared, after]) as Exclude<typeof quantity, number>)[0]!,
      before.storage?.warehouses ?? true)
    : before.scope === 'orders'
    ? orderCheckoutDifferences(before.state, prepared.state, compareState, quantity, refused, before.storage?.warehouses ?? true)
    : checkoutDifferences(before.state, prepared.state, compareState, quantity as number, refused);
  if (refused) differences.push(...(before.scope === 'orders'
    ? orderPurchaseDifferences(prepared.state, after.state, new Map([[before.state.accountId, 0]]), new Map(), before.storage?.warehouses ?? true)
    : purchaseDifferences(prepared.state, after.state, new Map([[before.state.accountId, 0]]), new Map())));
  const observation = { ...after, differences, before: input.before, prepared: input.prepared, ...(response ? { response } : {}) };
  if (differences[0]) {
    const { control, observed, expected } = differences[0];
    const value = finding('number-mismatch', { control, observed, expected: { equals: expected } });
    throw new ActionApplicationFailure(renderFinding(value), { finding: value, observation });
  }
  return observation;
}

async function dbExpectOperation({ input, capabilities }: ActionArguments<{
  before: string; otherBefore: string; actor: string;
  operation: 'buy' | 'cart-add' | 'cart-update' | 'checkout' | 'cancel' | 'restock' | 'transfer' | 'reconnect';
}>) {
  const database = capabilities['database-read'];
  const before = database.checkoutSnapshots.get(input.before), otherBefore = database.checkoutSnapshots.get(input.otherBefore);
  if (!before || !otherBefore) inconclusive('assertion-without-action', { action: 'dbRecordCheckout' });
  if (before.state.accountId === otherBefore.state.accountId) throw new Error('history needs two distinct customer accounts');
  const after = database.getCheckoutState(before), otherAfter = database.getCheckoutState(otherBefore);
  const catalog = (snapshot: CheckoutSnapshot) => JSON.stringify(snapshot.catalog?.toSorted((a, b) => a.itemId.localeCompare(b.itemId)));
  for (const snapshot of [before, otherBefore, after, otherAfter]) {
    if (snapshot.scope !== 'orders' || !snapshot.storage?.cart || !snapshot.storage.warehouses || !snapshot.catalog) {
      throw new Error('history requires native order, cart and warehouse observations');
    }
    if (JSON.stringify(snapshot.schemaSha256) !== JSON.stringify(before.schemaSha256)) throw new Error('history reader schema changed');
    if (catalog(snapshot) !== catalog(before)) fail('interface-invalid', { action: 'read orders', attribute: 'item', detail: 'operation changed the catalog' });
  }
  for (const [primary, peer] of [[before, otherBefore], [after, otherAfter]] as const) {
    const shared = { ...primary.state, accountId: peer.state.accountId, itemId: peer.state.itemId,
      priceMinor: peer.state.priceMinor, cart: peer.state.cart, reservations: peer.state.reservations };
    if (orderOperationDifferences(shared, peer.state, { kind: 'reconnect' }, before.catalog!).length) {
      const value = finding('invalid-input', { detail: 'history reads did not capture one quiescent shared state' });
      throw new ActionInconclusive(renderFinding(value), { finding: value, observation: { before, otherBefore, after, otherAfter } });
    }
  }
  const history = input.operation === 'reconnect' ? undefined : capabilities['named-actions'].lastCalls.get();
  const call = history?.outcomes[0];
  if (input.operation !== 'reconnect' && (!history || !call)) inconclusive('assertion-without-action', { action: 'callConcurrently' });
  if (history && (history.fired !== 1 || history.outcomes.length !== 1 || call!.name !== input.actor || call!.action !== input.operation)) {
    throw new Error('history comparison requires the matching single operation');
  }
  if (call && (call.complete !== true || !call.status || call.status === 202)) inconclusive('transport-incomplete', {});
  if (call && (!before.recordedAtMs || !otherBefore.recordedAtMs || !call.startedAtMs || !call.completedAtMs
    || call.startedAtMs < Math.max(before.recordedAtMs, otherBefore.recordedAtMs) || call.completedAtMs < call.startedAtMs)) {
    inconclusive('invalid-input', { detail: 'history response is missing timing or predates the snapshots' });
  }
  const values = call?.values;
  const quantity = () => {
    if (typeof values?.quantity !== 'number' || !Number.isSafeInteger(values.quantity)) throw new Error('history request has no exact quantity');
    return values.quantity;
  };
  let operation: OrderOperation;
  switch (input.operation) {
    case 'buy': operation = { kind: 'buy', itemId: checkoutId(values?.itemId) }; break;
    case 'cart-add': case 'cart-update': {
      const itemId = checkoutId(values?.itemId);
      operation = { kind: 'cart', itemId, quantity: input.operation === 'cart-update' ? quantity()
        : (before.state.cart.find(row => row.itemId === itemId)?.quantity ?? 0) + 1 };
      if (input.operation === 'cart-add' && values?.quantity !== undefined && values.quantity !== 1) throw new Error('history add uses one unit');
      break;
    }
    case 'cancel': operation = { kind: 'cancel', orderId: checkoutId(values?.orderId) }; break;
    case 'restock': operation = { kind: 'restock', itemId: checkoutId(values?.itemId), warehouseId: checkoutId(values?.warehouseId), quantity: quantity() }; break;
    case 'transfer': operation = { kind: 'transfer', itemId: checkoutId(values?.itemId), fromWarehouseId: checkoutId(values?.fromWarehouseId), toWarehouseId: checkoutId(values?.toWarehouseId), quantity: quantity() }; break;
    default: operation = { kind: input.operation };
  }
  const differences = orderOperationDifferences(before.state, after.state, operation, before.catalog!);
  differences.push(...orderOperationDifferences({ ...otherBefore.state, stock: after.state.stock, orders: after.state.orders },
    otherAfter.state, { kind: 'reconnect' }, before.catalog!).map(row => ({ ...row, control: `other customer: ${row.control}` })));
  const alreadyCancelled = operation.kind === 'cancel' && before.state.orders.some(row => row.id === operation.orderId && row.status === 'cancelled');
  if (call && !call.ok && !alreadyCancelled) differences.push({ control: 'valid history operation accepted', observed: 0, expected: 1 });
  const observation = { before: input.before, otherBefore: input.otherBefore, operation, ...(call ? { call } : {}), after, otherAfter, differences };
  if (differences[0]) {
    const { control, observed, expected } = differences[0];
    const value = finding('number-mismatch', { control, observed, expected: { equals: expected } });
    throw new ActionApplicationFailure(renderFinding(value), { finding: value, observation });
  }
  return observation;
}

async function dbExpectCancellation({ input, capabilities }: ActionArguments<{
  before: string; shipping?: 'wins' | 'competes';
}>) {
  const database = capabilities['database-read'];
  const before = database.checkoutSnapshots.get(input.before);
  if (!before) inconclusive('assertion-without-action', { action: 'dbRecordCheckout' });
  if (before.scope === 'orders' && before.storage?.kind !== 'order-data') inconclusive('invalid-input', { detail: 'saved order cancellation has no qualified mapping' });
  if (before.storage && !before.storage.warehouses) throw new Error('cancellation reconciliation requires warehouse evidence');
  if (input.shipping && before.scope !== 'orders') throw new Error('shipping reconciliation requires native order-data evidence');
  const after = database.getCheckoutState(before);
  if (JSON.stringify(before.schemaSha256) !== JSON.stringify(after.schemaSha256)) {
    throw new Error('checkout reader schema changed during cancellation');
  }
  if (before.scope !== after.scope) throw new Error('checkout scope changed during cancellation');
  const differences = before.scope === 'orders'
    ? orderCancellationDifferences(before.state, after.state, input.shipping)
    : cancellationDifferences(before.state, after.state);
  const observation = { ...after, differences, before: input.before };
  if (differences[0]) {
    const { control, observed, expected } = differences[0];
    const value = finding('number-mismatch', { control, observed, expected: { equals: expected } });
    throw new ActionApplicationFailure(renderFinding(value), { finding: value, observation });
  }
  return observation;
}

async function dbExpectNoPurchase({ input, capabilities }: ActionArguments<{ before: string }>) {
  const database = capabilities['database-read'];
  const before = database.checkoutSnapshots.get(input.before);
  if (!before) inconclusive('assertion-without-action', { action: 'dbRecordCheckout' });
  const after = database.getCheckoutState(before);
  if (before.scope !== after.scope || JSON.stringify(before.schemaSha256) !== JSON.stringify(after.schemaSha256)) {
    throw new Error('purchase reader changed during the test');
  }
  const compare = before.scope === 'orders' ? orderPurchaseDifferences : purchaseDifferences;
  const differences = compare(before.state, after.state, new Map([[before.state.accountId, 0]]), new Map());
  const observation = { ...after, before: input.before, differences };
  if (differences[0]) {
    const { control, observed, expected } = differences[0];
    const value = finding('number-mismatch', { control, observed, expected: { equals: expected } });
    throw new ActionApplicationFailure(renderFinding(value), { finding: value, observation });
  }
  return observation;
}

async function dbExpectPurchase({ input, capabilities }: ActionArguments<{ before: string; actor: string; stockBefore: string }>) {
  const database = capabilities['database-read'];
  const before = database.checkoutSnapshots.get(input.before);
  const stockBefore = capabilities['browser-observation'].recorded.get(input.stockBefore);
  const call = actorFor(capabilities, input.actor).actionCall;
  if (!before || stockBefore === undefined) inconclusive('assertion-without-action', { action: 'dbRecordCheckout/dbRecordStock' });
  if (!call) inconclusive('assertion-without-action', { action: 'callAction' });
  if (call.complete === false || !call.status) inconclusive('transport-incomplete', {});
  if (call.action !== 'buy' || before.scope !== 'orders' || before.storage?.kind !== 'order-data') {
    throw new Error('direct purchase reconciliation requires a buy response and order-data snapshot');
  }
  const after = database.getCheckoutState(before);
  if (after.scope !== before.scope || JSON.stringify(after.schemaSha256) !== JSON.stringify(before.schemaSha256)) {
    throw new Error('purchase reader changed during the test');
  }
  const count = call.accepted ? 1 : 0;
  const differences = orderPurchaseDifferences(before.state, after.state,
    new Map([[before.state.accountId, count]]), new Map(), before.storage.warehouses);
  const stock = await database.getStock({ item: before.item });
  if (stock.quantity !== stockBefore - count) differences.push({
    control: 'purchase stock', observed: stock.quantity, expected: stockBefore - count,
  });
  const observation = { ...after, before: input.before, response: call, stock, differences };
  if (differences[0]) {
    const { control, observed, expected } = differences[0];
    const value = finding('number-mismatch', { control, observed, expected: { equals: expected } });
    throw new ActionApplicationFailure(renderFinding(value), { finding: value, observation });
  }
  return observation;
}

async function dbExpectPurchaseCount({ input, capabilities }: ActionArguments<{
  before: string[]; purchasesEach: number;
}>) {
  const database = capabilities['database-read'];
  const snapshots = input.before.map(key => {
    const before = database.checkoutSnapshots.get(key);
    if (!before) inconclusive('assertion-without-action', { action: 'dbRecordCheckout' });
    if (before.scope !== 'orders' || before.storage?.kind !== 'order-data') throw new Error('purchase count requires native order data');
    return { key, before };
  });
  const accepted = new Map(snapshots.map(({ before }) => [before.state.accountId, input.purchasesEach]));
  if (accepted.size !== snapshots.length) throw new Error('purchase count requires distinct accounts');
  if (snapshots.some(({ before }) => before.state.itemId !== snapshots[0]!.before.state.itemId
    || before.state.priceMinor !== snapshots[0]!.before.state.priceMinor)) throw new Error('purchase count requires the same product and price');
  const after = snapshots.map(({ key, before }) => {
    const value = database.getCheckoutState(before);
    if (value.scope !== before.scope || JSON.stringify(value.schemaSha256) !== JSON.stringify(before.schemaSha256)) {
      throw new Error('purchase reader changed during the test');
    }
    return { key, ...value, differences: orderPurchaseDifferences(before.state, value.state,
      accepted, new Map(), before.storage!.warehouses) };
  });
  const observation = { before: input.before, purchasesEach: input.purchasesEach, after };
  const difference = after.flatMap(row => row.differences)[0];
  if (difference) {
    const value = finding('number-mismatch', { control: difference.control,
      observed: difference.observed, expected: { equals: difference.expected } });
    throw new ActionApplicationFailure(renderFinding(value), { finding: value, observation });
  }
  return observation;
}

async function dbExpectPurchases({ input, capabilities }: ActionArguments<{
  before: Record<string, string>; purchases: number;
}>) {
  const database = capabilities['database-read'];
  const history = capabilities['named-actions'].lastCalls.get();
  if (!history) inconclusive('assertion-without-action', { action: 'callConcurrently' });
  if (history.fired !== history.outcomes.length || history.outcomes.some(row => row.status === 0)) {
    inconclusive('transport-incomplete', {});
  }
  const snapshots = Object.entries(input.before).map(([actor, key]) => {
    const before = database.checkoutSnapshots.get(key);
    if (!before) inconclusive('assertion-without-action', { action: 'dbRecordCheckout' });
    if (before.scope === 'orders' && before.storage?.kind !== 'order-data') inconclusive('invalid-input', { detail: 'saved direct-purchase histories have no qualified mapping' });
    if (before.storage && !before.storage.warehouses) throw new Error('purchase reconciliation requires warehouse evidence');
    return { actor, before };
  });
  const accepted = new Map(snapshots.map(({ before }) => [before.state.accountId, 0]));
  if (accepted.size !== snapshots.length) throw new Error('purchase buyers must have distinct accounts');
  const restocked = new Map<string, number>();
  const differences: Array<{ control: string; observed: number; expected: number }> = [];
  for (const row of history.outcomes) {
    if (row.action === 'buy') {
      const buyer = snapshots.find(snapshot => snapshot.actor === row.name);
      if (!buyer || checkoutId(row.values?.itemId) !== buyer.before.state.itemId) throw new Error('purchase history selects different data');
      if (row.ok) accepted.set(buyer.before.state.accountId, accepted.get(buyer.before.state.accountId)! + 1);
    } else if (row.action === 'restock') {
      if (checkoutId(row.values?.itemId) !== snapshots[0]!.before.state.itemId) throw new Error('restock history selects different item');
      const warehouse = checkoutId(row.values?.warehouseId), quantity = row.values?.quantity;
      if (typeof quantity !== 'number' || !Number.isSafeInteger(quantity) || quantity < 1) throw new Error('invalid restock request quantity');
      const total = (restocked.get(warehouse) ?? 0) + quantity;
      if (!Number.isSafeInteger(total)) throw new Error('inexact restock total');
      restocked.set(warehouse, total);
      if (!row.ok) differences.push({ control: 'restock accepted', observed: 0, expected: 1 });
    } else throw new Error('purchase reconciliation requires buy/restock request history');
  }
  const count = [...accepted.values()].reduce((sum, value) => sum + value, 0);
  if (count !== input.purchases) differences.push({ control: 'purchase progress', observed: count, expected: input.purchases });
  const after = snapshots.map(({ actor, before }) => {
    const value = database.getCheckoutState(before);
    if (JSON.stringify(value.schemaSha256) !== JSON.stringify(before.schemaSha256)) throw new Error('purchase reader schema changed');
    if (value.scope !== before.scope) throw new Error('checkout scope changed during purchase');
    const compare = before.scope === 'orders' ? orderPurchaseDifferences : purchaseDifferences;
    differences.push(...compare(before.state, value.state, accepted, restocked));
    return { actor, ...value };
  });
  const observation = { before: input.before, after, differences, history };
  if (differences[0]) {
    const { control, observed, expected } = differences[0];
    const value = finding('number-mismatch', { control, observed, expected: { equals: expected } });
    throw new ActionApplicationFailure(renderFinding(value), { finding: value, observation });
  }
  return observation;
}

async function dbExpectStock({ input, capabilities, signal }: ActionArguments<ReadStockInput>) {
  const base = input.relativeTo === undefined ? undefined
    : capabilities['browser-observation'].recorded.get(input.relativeTo);
  if (input.relativeTo !== undefined && base === undefined) {
    inconclusive('assertion-without-action', { action: 'dbRecordStock' });
  }
  const equals = input.relativeTo === undefined ? input.equals : base! + (input.plus ?? 0);
  const expected = { ...(equals === undefined ? {} : { equals }),
    ...(input.atLeast === undefined ? {} : { atLeast: input.atLeast }),
    ...(input.atMost === undefined ? {} : { atMost: input.atMost }) };
  if (!Object.keys(expected).length || Object.values(expected).some(value => !Number.isSafeInteger(value))) {
    throw new Error('expected stock is not an exact integer');
  }
  const deadline = Date.now() + (input.within ?? 0);
  let value = await capabilities['database-read'].getStock(input);
  while (!numberMatches(value.quantity, expected) && Date.now() < deadline) {
    await capabilities.clock.sleep(Math.min(250, deadline - Date.now()), signal);
    value = await capabilities['database-read'].getStock(input);
  }
  if (!numberMatches(value.quantity, expected)) fail('number-mismatch', {
    control: `stored stock for ${input.item}${input.warehouse ? ` in ${input.warehouse}` : ''}`,
    observed: value.quantity, expected,
  });
  return { ...value, expected };
}

interface ProcessErrorShape {
  readonly orderDataInterface?: unknown;
  readonly name?: unknown;
  readonly classification?: unknown;
  readonly code?: unknown;
  readonly message?: unknown;
  readonly status?: unknown;
  readonly stderr?: unknown;
  readonly stdout?: unknown;
  readonly stockInterface?: unknown;
  readonly stockInterfaceInvalid?: unknown;
  readonly missingRow?: unknown;
}

interface NestedActionEvidence {
  readonly status?: unknown;
  readonly summary?: string | null;
  readonly finding?: unknown;
}

const errorShape = (error: unknown): ProcessErrorShape =>
  error !== null && typeof error === 'object' ? error as ProcessErrorShape : {};
const errorEvidence = (error: unknown): NestedActionEvidence | null => {
  if (error === null || typeof error !== 'object' || !('actionEvidence' in error)) return null;
  const evidence = error.actionEvidence;
  return evidence !== null && typeof evidence === 'object' ? evidence as NestedActionEvidence : null;
};

export function databaseWriteFailureDetail(error: unknown): string {
  const value = errorShape(error);
  const details = [value.message, value.stdout, value.stderr]
    .map(value => Buffer.isBuffer(value) ? value.toString('utf8') : String(value ?? ''))
    .map(value => value.trim()).filter(Boolean);
  return redactCredentials([...new Set(details)].join(' | ')).slice(-600) || 'unknown database-write failure';
}

async function dispatchNested(
  concurrency: ConcurrencyCapability,
  step: ActionStep,
  signal: AbortSignal,
): Promise<unknown> {
  try {
    return await concurrency.dispatch(step, signal);
  } catch (error) {
    const evidence = errorEvidence(error);
    const status = evidence?.status;
    const disposition = typeof status === 'string'
      ? evidenceDisposition(status as CheckEvidenceStatus)
      : null;
    // A nested step's own finding is the finding; the wrapper adds nothing.
    const nested = isFinding(evidence?.finding) ? evidence.finding : null;
    if (disposition?.applicationFailure) {
      throw new ActionApplicationFailure(evidence?.summary ?? `${step.do} failed`,
        { finding: nested ?? finding('action-failed', { action: step.do }) });
    }
    if (disposition?.outcomeKind === 'inconclusive') {
      throw new ActionInconclusive(evidence?.summary ?? `${step.do} was inconclusive`,
        { finding: nested ?? finding('invalid-input', { detail: `${step.do} was inconclusive` }) });
    }
    throw error;
  }
}

// Drain every branch before another criterion can use the app. An early app
// failure must not hide a later harness failure or leave writes in flight.
async function settleConcurrentActions<T>(pending: readonly Promise<T>[]): Promise<T[]> {
  const settled = await Promise.allSettled(pending);
  const failures = settled.filter(result => result.status === 'rejected');
  const failure = failures.find(result => !(result.reason instanceof ActionApplicationFailure)
    && !(result.reason instanceof ActionInconclusive))
    ?? failures.find(result => result.reason instanceof ActionInconclusive) ?? failures[0];
  if (failure) throw failure.reason;
  return settled.map(result => (result as PromiseFulfilledResult<T>).value);
}

async function replayConcurrently(
  { input, capabilities, signal }: ActionArguments<ReplayConcurrentlyInput>,
) {
  const concurrency = capabilities.concurrency;
  const method = input.method ?? 'POST';
  const match = input.match;
  const pick = (actor: Actor): CapturedWrite | undefined | null => {
    if (match) {
      return [...(actor.writes ?? [])].reverse()
        .find(write => write.method === method && write.url.includes(match));
    }
    return actor.lastWrites?.[method] ?? actor.lastWrite;
  };
  const pending = input.actors.map(name => {
    const actor = actorFor(capabilities, name);
    return { actor, write: pick(actor) };
  }).filter((candidate): candidate is { actor: Actor; write: CapturedWrite } =>
    candidate.write !== null && candidate.write !== undefined);
  if (pending.length < 2) {
    inconclusive('nothing-contended', { detail: `${pending.length} write request(s) captured; `
      + 'this backend may not write over HTTP, or the request carried no JSON body' });
  }
  const replies = await Promise.all(pending.map(({ actor, write }) =>
    actor.page.request.fetch(write.url, {
      method: write.method,
      headers: replayHeaders(write),
      data: write.body === undefined || write.body === null ? undefined : JSON.stringify(write.body),
    }).then(response => response.status(), error =>
      `error: ${String(errorShape(error).message ?? error).split('\n')[0]}`)));
  const answered = replies.filter(status => typeof status === 'number');
  if (answered.length < 2) {
    inconclusive('nothing-contended', { detail: `${answered.length} of ${pending.length} replayed requests `
      + `reached the server (responses: ${replies.join(', ')})` });
  }
  await concurrency.sleep(input.settleMs ?? 3000, signal);
  return { attempted: pending.length, answered: answered.length, replies };
}

async function clickConcurrently(
  { input, capabilities, signal }: ActionArguments<ClickConcurrentlyInput>,
) {
  const concurrency = capabilities.concurrency;
  const targets: readonly ClickTarget[] = input.targets ?? input.actors.map(actor => ({ actor }));
  const resolved = targets.map(target => {
    const where = target.in ?? input.in;
    const scope = where
      ? { testid: where.testid, contains: concurrency.expand(where.contains) }
      : undefined;
    return { target, locator: actorFor(capabilities, target.actor).loc(input.testid, { scope }) };
  });
  const notReady = (await settleConcurrentActions(resolved.map(async ({ target, locator }) => {
    try {
      await locator.waitFor({ state: 'visible', timeout: input.readyWithin ?? 15000 });
      return await locator.isEnabled() ? null : target.actor;
    } catch (error) {
      if (errorShape(error).name !== 'TimeoutError') throw error;
      return target.actor;
    }
  }))).filter(Boolean);
  if (notReady.length) {
    fail('control-not-ready', { control: input.testid, actors: notReady.map(String) });
  }
  const outcomes = await settleConcurrentActions(resolved.map(({ target, locator }) =>
    locator.click({ timeout: input.within ?? concurrency.defaultWithin, force: true, noWaitAfter: true })
      .then(() => null, error => {
        if (errorShape(error).name !== 'TimeoutError') throw error;
        return `${target.actor}: ${String(errorShape(error).message ?? error).split('\n')[0]}`;
      })));
  const failed = outcomes.filter(Boolean);
  if (failed.length) {
    fail('clicks-failed', { control: input.testid, failed: failed.length, total: targets.length,
      detail: failed.join(' | ') });
  }
  await concurrency.sleep(input.settleMs ?? 3000, signal);
  return { dispatched: targets.length };
}

async function race({ input, capabilities, signal }: ActionArguments<RaceInput>) {
  const concurrency = capabilities.concurrency;
  await settleConcurrentActions(input.branches.map(async branch => {
    for (const step of branch) await dispatchNested(concurrency, step, signal);
  }));
  await concurrency.sleep(input.settleMs ?? 2000, signal);
  return { branches: input.branches.length };
}

async function sendConcurrently(
  { input, capabilities, signal }: ActionArguments<SendConcurrentlyInput>,
) {
  const concurrency = capabilities.concurrency;
  await settleConcurrentActions(input.senders.map(sender => dispatchNested(concurrency, {
    do: 'sendMany',
    actor: sender.actor,
    prefix: sender.prefix,
    count: sender.count,
    delayMs: sender.delayMs ?? input.delayMs ?? 0,
  }, signal)));
  return { senders: input.senders.length,
    messages: input.senders.reduce((total, sender) => total + sender.count, 0) };
}

async function restartBackend({ input, capabilities, signal }: ActionArguments<SettleInput>) {
  await capabilities['backend-lifecycle'].operate('restart', input.settleMs ?? 10000, signal);
  return { operation: 'restart' };
}

async function startAppServer({ input, capabilities, signal }: ActionArguments<SettleInput>) {
  await capabilities['application-lifecycle'].operate('start', input.settleMs ?? 8000, signal);
  return { operation: 'start' };
}

async function stopAppServer({ input, capabilities, signal }: ActionArguments<SettleInput>) {
  await capabilities['application-lifecycle'].operate('stop', input.settleMs ?? 2000, signal);
  return { operation: 'stop' };
}

async function dbSetStock({ input, capabilities, signal }: ActionArguments<SetStockInput>) {
  let result: unknown;
  try {
    result = await capabilities['database-write'].setStock(input);
  } catch (error) {
    if (errorShape(error).classification) throw error;
    // The contract names the stock tables; an application without them has
    // failed that interface. Any other write failure is the harness's.
    if (errorShape(error).stockInterface === true) {
      const row = errorShape(error).missingRow;
      fail('stock-interface-missing', { detail: databaseWriteFailureDetail(error), item: input.item, warehouse: input.warehouse,
        ...(row === 'item' || row === 'warehouse' || row === 'stock' ? { missingRow: row } : {}) });
    }
    throw new Error(`direct database write failed: ${databaseWriteFailureDetail(error)}`, { cause: error });
  }
  await capabilities.clock.sleep(input.settleMs, signal);
  return result;
}

async function setOffline({ input, capabilities, signal }: ActionArguments<OfflineInput>) {
  const actor = actorFor(capabilities, input.actor);
  const browser = capabilities['browser-interaction'];
  const offline = input.offline !== false;
  // Emulation alone pauses an open WebSocket and later delivers what it held.
  if (!actor.networkInterruption) {
    inconclusive('network-not-interrupted', { actor: input.actor,
      detail: 'this client was not opened with an interruptible network' });
  }
  let connections: { closed: number; open: string[] } | undefined;
  if (offline) {
    // The device goes offline first, as a real outage reports; then its connections are cut.
    await actor.page.context().setOffline(true);
    connections = await actor.networkInterruption.interrupt();
    if (connections.open.length) {
      inconclusive('network-not-interrupted', { actor: input.actor,
        detail: `still open after the interruption: ${connections.open.slice(0, 3).join('; ')}` });
    }
  } else {
    // Reopen forwarding while emulation still blocks the page, then restore the page's network.
    await actor.networkInterruption.restore();
    await actor.page.context().setOffline(false);
  }
  await browser.sleep(input.settleMs ?? 500, signal);
  const browserOnline = await actor.page.evaluate(() => navigator.onLine);
  if (browserOnline === offline) {
    throw new Error(`setOffline requested browser network ${offline ? 'offline' : 'online'}, `
      + `but navigator.onLine remained ${browserOnline}`);
  }
  return { offline, browserOnline, ...connections };
}

async function closeClient({ input, capabilities }: ActionArguments<ActorInput>) {
  await actorFor(capabilities, input.actor).page.close();
  return { closed: true };
}

async function openClient({ input, capabilities, signal }: ActionArguments<ActorInput>) {
  const actor = actorFor(capabilities, input.actor);
  await capabilities['browser-interaction'].clients.open(actor, input.settleMs ?? 4000, signal);
  return { opened: true };
}

async function freshClient({ input, capabilities }: ActionArguments<ActorInput & { preserveStorage?: boolean }>) {
  const actor = actorFor(capabilities, input.actor);
  const name = await capabilities['browser-interaction'].clients.fresh(actor, input.actor, input.preserveStorage ?? false);
  return { actor: name };
}

interface LifecycleCapabilityOptions {
  readonly target: 'app-server' | 'backend-runtime';
  readonly control: (
    restartSpec: RuntimeControlSpec,
    mode: RuntimeControlMode,
    options: { readonly signal: AbortSignal },
  ) => Promise<unknown>;
  readonly restartSpec?: RuntimeControlSpec;
  readonly sleep: Sleep;
  // Called after the control command completes, so the caller knows which
  // state the harness left the target in.
  readonly onOperated?: (mode: 'restart' | 'start' | 'stop') => void;
}

export function createLifecycleCapability({ restartSpec, target,
  sleep, control, onOperated = () => {},
}: LifecycleCapabilityOptions): LifecycleCapability {
  const application = target === 'app-server';
  return Object.freeze({
    async operate(mode: 'restart' | 'start' | 'stop', settleMs: number, signal: AbortSignal) {
      if (!restartSpec) {
        inconclusive('no-backend-control', { target });
        return;
      }
      try {
        await control(restartSpec, mode, { signal });
      } catch (error) {
        const value = errorShape(error);
        if (value.status === 3) inconclusive('control-refused', { target });
        // App faults carry generated_app_not_restartable. Otherwise a generated app
        // that cannot complete its own start/stop has failed the contract, but the
        // backend runtime and harness command failures provide no app evidence.
        const appFault = value.code === 'generated_app_not_restartable'
          || (application && !harnessProcessFailure(error));
        if (!appFault) throw error;
        fail('app-control-failed', { mode, target,
          detail: String(value.stdout || value.message || '').trim().slice(-200) });
      }
      onOperated(mode);
      await sleep(settleMs, signal);
    },
  });
}

interface DatabaseWriteCapabilityOptions {
  readonly backend?: string | null;
  readonly databaseLease?: LeasedDatabase | null;
  readonly skip?: boolean;
  readonly exec?: Exec;
  readonly expand: (value: string) => string;
  readonly spacetime?: LeasedSpacetimeTarget | null;
}

export function createDatabaseWriteCapability({ backend, spacetime, databaseLease, skip = false, expand,
  exec = execFileSync }: DatabaseWriteCapabilityOptions) {
  return Object.freeze({
    setStock(input: SetStockInput): unknown {
      if (skip) return { skipped: true };
      const item = expand(input.item);
      const warehouse = expand(input.warehouse);
      const quantity = Number(input.quantity);
      if (!Number.isInteger(quantity)) {
        inconclusive('invalid-input', { detail: `dbSetStock quantity ${input.quantity} is not a whole number` });
      }
      const adapter = backend ? STACK_ADAPTER_REGISTRY.get(backend) : undefined;
      if (!adapter || !('databaseWrite' in adapter)) {
        return inconclusive('unsupported-backend', { backend: backend ?? '<unset>' });
      }
      if (adapter.id === 'spacetime') {
        return adapter.databaseWrite.setStock({ item, warehouse, quantity, spacetime: spacetime ?? undefined, exec });
      }
      if (adapter.id === 'convex') return adapter.databaseWrite.setStock({ item, warehouse, quantity, exec });
      if (!databaseLease) {
        throw Object.assign(new Error('direct database writes require an authenticated backend lease'),
          { classification: 'harness_failure' });
      }
      return adapter.databaseWrite.setStock({ item, warehouse, quantity, lease: databaseLease, exec });
    },
  });
}

export function createDatabaseReadCapability({ backend, spacetime, databaseLease, skip = false, expand, app,
  savedReader, contractIds,
  checkoutSnapshots = new Map<string, CheckoutSnapshot & { account: string; item: string }>(),
  checkoutActivity = { unsettled: false },
  exec = execFileSync }: DatabaseWriteCapabilityOptions & { app?: string;
    contractIds?: readonly string[];
    savedReader?: { path: string; sha256: string };
    checkoutActivity?: { unsettled: boolean };
    checkoutSnapshots?: Map<string, CheckoutSnapshot & { account: string; item: string }> }) {
  const requireSettled = () => { if (checkoutActivity.unsettled) inconclusive('transport-incomplete', {}); };
  return Object.freeze({
    checkoutSnapshots,
    // A client disconnect cannot stop an HTTP handler retrying after a DB crash.
    // This grade cannot safely compare later global state; reset in a new grade.
    markCheckoutUnsettled() { checkoutActivity.unsettled = true; },
    getCheckoutState(input: { account: string; item: string; storage?: OrderDataSelection }): CheckoutSnapshot {
      requireSettled();
      if (skip) throw new Error('checkout state reads are disabled for this control');
      if (!app) throw new Error('checkout state reads require a verified application source directory');
      const adapter = backend ? STACK_ADAPTER_REGISTRY.get(backend) : undefined;
      if (!adapter || !('databaseRead' in adapter)) inconclusive('unsupported-backend', { backend: backend ?? '<unset>' });
      const selection = { account: expand(input.account), item: expand(input.item), app, exec,
        storage: input.storage ? resolveOrderDataStorage(input.storage, contractIds) : undefined };
      if (savedReader) {
        if (input.storage) throw new Error('saved schema mapping cannot replace the declared order data interface');
        if (backend === 'spacetime') return getSavedSpacetimeCheckoutState({ ...selection, reader: savedReader, spacetime: spacetime ?? undefined });
        if (!databaseLease) throw new Error('saved order reader requires an authenticated backend lease');
        const read = backend === 'postgres' ? getSavedPostgresCheckoutState : getSavedMongoDbCheckoutState;
        return read({ ...selection, reader: savedReader, lease: databaseLease });
      }
      try {
        if (adapter.id === 'convex') return adapter.databaseRead.getCheckoutState(selection);
        if (adapter.id === 'spacetime') return adapter.databaseRead.getCheckoutState({ ...selection, spacetime: spacetime ?? undefined });
        if (!databaseLease) throw new Error('checkout state reads require an authenticated backend lease');
        return adapter.databaseRead.getCheckoutState({ ...selection, lease: databaseLease });
      } catch (error) {
        if (errorShape(error).orderDataInterface === true) {
          fail('interface-invalid', { action: 'read orders', attribute: 'order data', detail: databaseWriteFailureDetail(error) });
        }
        throw error;
      }
    },
    getStock(input: { item: string; warehouse?: string }) {
      requireSettled();
      if (skip) inconclusive('stock-read-unavailable', { detail: 'direct stock reads are disabled for this control' });
      const item = expand(input.item);
      const warehouse = input.warehouse === undefined ? undefined : expand(input.warehouse);
      const adapter = backend ? STACK_ADAPTER_REGISTRY.get(backend) : undefined;
      if (!adapter || !('databaseRead' in adapter)) {
        return inconclusive('unsupported-backend', { backend: backend ?? '<unset>' });
      }
      try {
        if (adapter.id !== 'spacetime' && adapter.id !== 'convex' && !databaseLease) throw Object.assign(
          new Error('direct database reads require an authenticated backend lease'),
          { classification: 'harness_failure' });
        const value = adapter.id === 'convex'
          ? adapter.databaseRead.getStock({ item, warehouse, exec }) : adapter.id === 'spacetime'
          ? adapter.databaseRead.getStock({ item, warehouse, spacetime: spacetime ?? undefined, exec })
          : adapter.databaseRead.getStock({ item, warehouse, lease: databaseLease!, exec });
        if (!Number.isSafeInteger(value.quantity)) {
          inconclusive('stock-read-unavailable', { detail: 'stored stock did not return an exact integer quantity' });
        }
        return value;
      } catch (error) {
        if (errorShape(error).classification) throw error;
        if (errorShape(error).stockInterfaceInvalid === true) {
          fail('interface-invalid', { action: 'read stock', attribute: 'stock data',
            detail: databaseWriteFailureDetail(error) });
        }
        if (errorShape(error).stockInterface === true) {
          const row = errorShape(error).missingRow;
          fail('stock-interface-missing', { detail: databaseWriteFailureDetail(error), item, warehouse,
            ...(row === 'item' || row === 'warehouse' || row === 'stock' ? { missingRow: row } : {}) });
        }
        throw new Error(`direct database read failed: ${databaseWriteFailureDetail(error)}`, { cause: error });
      }
    },
  });
}

function contractLifecycleAction<Input, Result>(
  implementation: (arguments_: ActionArguments<Input>) => Result | Promise<Result>,
): ActionImplementation {
  return actionImplementation(implementation);
}

function contractBrowserLifecycleAction<Input, Result>(
  implementation: (arguments_: ActionArguments<Input>) => Result | Promise<Result>,
): ActionImplementation {
  return contractLifecycleAction(browserApplicationBoundary(implementation));
}

export const RUNTIME_ACTION_IMPLEMENTATIONS = Object.freeze({
  dbRecordCheckout: contractLifecycleAction(dbRecordCheckout),
  dbExpectCatalogItem: contractLifecycleAction(dbExpectCatalogItem),
  dbExpectCheckout: contractLifecycleAction(dbExpectCheckout),
  dbExpectOperation: contractLifecycleAction(dbExpectOperation),
  dbExpectCancellation: contractLifecycleAction(dbExpectCancellation),
  dbExpectNoPurchase: contractLifecycleAction(dbExpectNoPurchase),
  dbExpectPurchase: contractLifecycleAction(dbExpectPurchase),
  dbExpectPurchases: contractLifecycleAction(dbExpectPurchases),
  dbExpectPurchaseCount: contractLifecycleAction(dbExpectPurchaseCount),
  dbRecordStock: contractLifecycleAction(dbRecordStock),
  dbExpectStock: contractLifecycleAction(dbExpectStock),
  clickConcurrently: contractLifecycleAction(clickConcurrently),
  closeClient: contractBrowserLifecycleAction(closeClient),
  dbSetStock: contractLifecycleAction(dbSetStock),
  freshClient: contractBrowserLifecycleAction(freshClient),
  openClient: contractBrowserLifecycleAction(openClient),
  race: contractLifecycleAction(race),
  replayConcurrently: contractBrowserLifecycleAction(replayConcurrently),
  restartBackend: contractLifecycleAction(restartBackend),
  sendConcurrently: contractLifecycleAction(sendConcurrently),
  setOffline: contractBrowserLifecycleAction(setOffline),
  startAppServer: contractLifecycleAction(startAppServer),
  stopAppServer: contractLifecycleAction(stopAppServer),
});
