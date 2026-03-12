import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { Identity } from 'spacetimedb';
import {
  DbConnection,
  ErrorContext,
  EventContext,
  tables,
} from './module_bindings/index.js';

type PromiseWithResolvers = <T>() => {
  promise: Promise<T>;
  resolve: (value: T | PromiseLike<T>) => void;
  reject: (reason?: unknown) => void;
};

function ensurePromiseWithResolvers(): void {
  const promiseCtor = Promise as PromiseConstructor & {
    withResolvers?: PromiseWithResolvers;
  };

  if (typeof promiseCtor.withResolvers === 'function') {
    return;
  }

  promiseCtor.withResolvers = function <T>() {
    let resolve!: (value: T | PromiseLike<T>) => void;
    let reject!: (reason?: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    return { promise, resolve, reject };
  };
}

ensurePromiseWithResolvers();

const HOST = process.env.SPACETIMEDB_HOST ?? 'ws://localhost:3000';
const DB_NAME = process.env.SPACETIMEDB_DB_NAME ?? 'reducer-rtt-demo';
const intervalMsFromEnv = Number.parseInt(
  process.env.RTT_INTERVAL_MS ?? '2000',
  10
);
const RTT_INTERVAL_MS =
  Number.isFinite(intervalMsFromEnv) && intervalMsFromEnv > 0
    ? intervalMsFromEnv
    : 2000;

const PROBE_PREFIX = 'rtt-probe-';
const probeStartByName = new Map<string, bigint>();
let probeInterval: NodeJS.Timeout | undefined;

async function main(): Promise<void> {
  console.log('Connecting to SpacetimeDB...');
  console.log(`  URI: ${HOST}`);
  console.log(`  Module: ${DB_NAME}`);
  console.log(`  Probe interval: ${RTT_INTERVAL_MS}ms`);

  DbConnection.builder()
    .withUri(HOST)
    .withDatabaseName(DB_NAME)
    .onConnect(onConnect)
    .onDisconnect(onDisconnect)
    .onConnectError(onConnectError)
    .build();
}

function onConnect(
  conn: DbConnection,
  identity: Identity,
  token: string
): void {
  console.log('\nConnected to SpacetimeDB');
  console.log(`Identity: ${identity.toHexString().slice(0, 16)}...`);

  conn
    .subscriptionBuilder()
    .onApplied(() => {
      console.log('Subscription applied; starting RTT probes...');
      startProbeLoop(conn);
    })
    .onError((_ctx, err) => {
      console.error('Subscription error:', err);
    })
    .subscribe(tables.person);

  conn.db.person.onInsert((ctx: EventContext, person) => {
    if (!person.name.startsWith(PROBE_PREFIX)) {
      return;
    }
    if (ctx.event.tag !== 'Reducer') {
      return;
    }

    const reducerEvent = ctx.event.value;
    if (reducerEvent.reducer.name !== 'add') {
      return;
    }

    const startedAt = probeStartByName.get(person.name);
    if (!startedAt) {
      return;
    }

    probeStartByName.delete(person.name);
    const rttMs = Number(process.hrtime.bigint() - startedAt) / 1_000_000;
    const reducerTimestamp = reducerEvent.timestamp.toISOString();
    console.log(
      `[RTT] ${rttMs.toFixed(2)} ms | reducer=${reducerEvent.reducer.name} | at=${reducerTimestamp}`
    );
  });
}

function onDisconnect(_ctx: ErrorContext, error?: Error): void {
  stopProbeLoop();
  probeStartByName.clear();

  if (error) {
    console.error('Disconnected with error:', error);
  } else {
    console.log('Disconnected from SpacetimeDB');
  }
}

function onConnectError(_ctx: ErrorContext, error: Error): void {
  console.error(`Connection error while dialing ${HOST}`);
  console.error('Make sure the local server is running in another terminal:');
  console.error('  spacetime start');
  console.error(`Then publish the demo module as "${DB_NAME}":`);
  console.error('  cd demo/reducer-rtt-ts/spacetimedb');
  console.error(`  spacetime publish --server local ${DB_NAME}`);
  console.error('Original error:', error);
  process.exit(1);
}

function startProbeLoop(conn: DbConnection): void {
  stopProbeLoop();

  sendProbe(conn);
  probeInterval = setInterval(() => {
    sendProbe(conn);
  }, RTT_INTERVAL_MS);

  console.log('Press Ctrl+C to exit');
}

function stopProbeLoop(): void {
  if (probeInterval) {
    clearInterval(probeInterval);
    probeInterval = undefined;
  }
}

function sendProbe(conn: DbConnection): void {
  const probeName = `${PROBE_PREFIX}${Date.now()}-${Math.random()
    .toString(36)
    .slice(2, 8)}`;

  probeStartByName.set(probeName, process.hrtime.bigint());
  void conn.reducers.add({ name: probeName }).catch(err => {
    probeStartByName.delete(probeName);
    console.error('Probe reducer call failed:', err);
  });
}

main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
