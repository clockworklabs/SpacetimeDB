import hdr from 'hdr-histogram-js';
import { performance } from 'node:perf_hooks';
import { pickTwoDistinct, zipfSampler } from './zipf.ts';
import { getSpacetimeCommittedTransfers } from './spacetimeMetrics.ts';
import { makeCollisionTracker } from './collision_tracker.ts';
import { RunResult } from './types.ts';

const OP_TIMEOUT_MS = Number(process.env.BENCH_OP_TIMEOUT_MS ?? '15000');

async function withOpTimeout<T>(
  promise: Promise<T>,
  label: string,
): Promise<T> {
  const timeoutMs = OP_TIMEOUT_MS;
  let timer: NodeJS.Timeout | undefined;

  const timeoutPromise = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      reject(new Error(`[runOne] ${label} timed out after ${timeoutMs}ms`));
    }, timeoutMs);
  });

  try {
    return (await Promise.race([promise, timeoutPromise])) as T;
  } finally {
    if (timer) clearTimeout(timer);
  }
}

export async function runOne({
  connector,
  scenario,
  seconds,
  concurrency,
  accounts,
  alpha,
}: {
  connector: {
    name: string;
    open(workers?: number): Promise<void>;
    close: () => Promise<void>;
    verify: () => Promise<void>;
    createWorker?: (opts?: {
      index: number;
      total: number;
    }) => Promise<unknown>;
  } & Record<string, any>;
  scenario: (
    conn: unknown,
    from: number,
    to: number,
    amount: number,
  ) => Promise<void>;
  seconds: number;
  concurrency: number;
  accounts: number;
  alpha: number;
}): Promise<RunResult> {
  console.log(
    `[${connector.name}] Running ${seconds}s with ${concurrency} workers, ${accounts} accounts, alpha=${alpha}`,
  );

  const collisionTracker = makeCollisionTracker();

  const hist = hdr.build({
    lowestDiscernibleValue: 1,
    highestTrackableValue: 10_000_000_000,
    numberOfSignificantValueDigits: 3,
  });

  const hasWorkerFactory =
    typeof (connector as any).createWorker === 'function';

  const workers: unknown[] = [];

  if (hasWorkerFactory) {
    await connector.open(concurrency);

    const createWorker = (connector as any).createWorker as (opts?: {
      index: number;
      total: number;
    }) => Promise<unknown>;

    for (let i = 0; i < concurrency; i++) {
      const workerConn = await createWorker({ index: i, total: concurrency });
      workers.push(workerConn);
    }
  } else {
    await connector.open();
    for (let i = 0; i < concurrency; i++) {
      workers.push(connector);
    }
  }

  const useSpacetimeMetrics =
    process.env.USE_SPACETIME_METRICS_ENDPOINT === '1' &&
    connector.name === 'spacetimedb';
  let beforeTransfers: bigint | null = null;

  if (useSpacetimeMetrics) {
    try {
      beforeTransfers = await getSpacetimeCommittedTransfers();
      if (beforeTransfers !== null) {
        console.log(
          `[spacetimedb] metrics before run: committed transfer txns = ${beforeTransfers.toString()}`,
        );
      } else {
        beforeTransfers = BigInt(0);
        console.warn(
          '[spacetimedb] spacetime_num_txns_total (transfer) not found before run; falling back to client call count',
        );
      }
    } catch (err) {
      console.warn(
        '[spacetimedb] failed to read metrics before run; falling back to client call count:',
        err,
      );
    }
  }

  const pick = zipfSampler(accounts, alpha);
  const start = performance.now();
  const endAt = start + seconds * 1000;
  let completed = 0;

  const PIPELINED = process.env.BENCH_PIPELINED === '1';
  const MAX_INFLIGHT_ENV = process.env.MAX_INFLIGHT_PER_WORKER;
  const MAX_INFLIGHT_PER_WORKER =
    MAX_INFLIGHT_ENV === '0' ? Infinity : Number(MAX_INFLIGHT_ENV ?? '8');

  async function worker(workerIndex: number) {
    const conn = workers[workerIndex];

    if (!PIPELINED) {
      while (performance.now() < endAt) {
        const [from, to] = pickTwoDistinct(pick);

        collisionTracker.begin(from);
        collisionTracker.begin(to);

        const t0 = performance.now();
        try {
          await withOpTimeout(
            scenario(conn as unknown, from, to, 1),
            `${connector.name} scenario ${from}->${to}`,
          );
        } catch (err) {
          const msg =
            err instanceof Error ? `${err.name}: ${err.message}` : String(err);
          console.warn(
            `[${connector.name}] Scenario failed for ${from} -> ${to}: ${msg}`,
          );
        } finally {
          collisionTracker.end(from);
          collisionTracker.end(to);
        }

        const t1 = performance.now();
        hist.recordValue(Math.max(1, Math.round((t1 - t0) * 1e3)));
        completed++;
      }
      return;
    }

    const inflight = new Set<Promise<void>>();
    const unlimitedInflight = !Number.isFinite(MAX_INFLIGHT_PER_WORKER);

    const launchOp = () => {
      const [from, to] = pickTwoDistinct(pick);

      collisionTracker.begin(from);
      collisionTracker.begin(to);

      const t0 = performance.now();

      const p = (async () => {
        try {
          await withOpTimeout(
            scenario(conn as unknown, from, to, 1),
            `${connector.name} scenario ${from}->${to}`,
          );
        } catch (err) {
          const msg =
            err instanceof Error ? `${err.name}: ${err.message}` : String(err);
          console.warn(
            `[${connector.name}] Scenario failed for ${from} -> ${to}: ${msg}`,
          );
        } finally {
          collisionTracker.end(from);
          collisionTracker.end(to);

          const t1 = performance.now();
          hist.recordValue(Math.max(1, Math.round((t1 - t0) * 1e3)));
          completed++;
        }
      })();

      inflight.add(p);
      p.finally(() => {
        inflight.delete(p);
      });
    };

    while (performance.now() < endAt) {
      if (unlimitedInflight || inflight.size < MAX_INFLIGHT_PER_WORKER) {
        // unlimited: this is always true → just keep launching
        launchOp();
      } else {
        // bounded: yield to let existing inflight ops progress, but do NOT
        // wait for any specific one to finish
        await new Promise((resolve) => setTimeout(resolve, 0));
      }
    }

    // after the time window, wait for remaining ops to settle so stats are sane
    await Promise.all(inflight);
  }

  console.log(`[${connector.name}] Starting workers for ${seconds}s run...`);

  await Promise.all(Array.from({ length: concurrency }, (_, i) => worker(i)));

  console.log(
    `[${connector.name}] All workers finished; collecting metrics...`,
  );

  let committedDelta: number | null = null;

  if (useSpacetimeMetrics && beforeTransfers !== null) {
    try {
      const afterTransfers = await getSpacetimeCommittedTransfers();
      if (afterTransfers !== null && afterTransfers >= beforeTransfers) {
        const deltaBig = afterTransfers - beforeTransfers;
        const maxSafe = BigInt(Number.MAX_SAFE_INTEGER);
        committedDelta =
          deltaBig <= maxSafe ? Number(deltaBig) : Number(maxSafe);

        console.log(
          `[spacetimedb] metrics after run: committed transfer txns = ${afterTransfers.toString()} (delta = ${deltaBig.toString()})`,
        );
      } else {
        console.warn(
          '[spacetimedb] metrics after run missing or decreased; ignoring metrics delta',
        );
      }
    } catch (err) {
      console.warn(
        '[spacetimedb] failed to read metrics after run; ignoring metrics delta:',
        err,
      );
    }
  }

  if (process.env.VERIFY === '1') {
    console.log(`[${connector.name}] Running verification pass...`);
    try {
      await withOpTimeout(connector.verify(), `${connector.name} verify()`);
    } catch (err) {
      console.error(`[${connector.name}] Verification failed:`, err);
    }
  }

  if (hasWorkerFactory) {
    for (const w of workers) {
      const c = w as { close?: () => Promise<void> };
      if (typeof c.close === 'function') {
        try {
          await withOpTimeout(c.close(), `${connector.name} worker close`);
        } catch (err) {
          console.warn(
            `[${connector.name}] Worker close failed: ${
              err instanceof Error ? err.message : String(err)
            }`,
          );
        }
      }
    }
  }

  await withOpTimeout(connector.close(), `${connector.name} root close`);

  const q = (p: number) => hist.getValueAtPercentile(p) / 1000;

  const committedOrCompleted = committedDelta ?? completed;

  const c = collisionTracker.stats();

  return {
    tps: committedOrCompleted / seconds,
    samples: completed,
    committed_txns: committedDelta,
    p50_ms: q(50),
    p95_ms: q(95),
    p99_ms: q(99),
    collision_ops: c.total,
    collision_count: c.collisions,
    collision_rate: c.collisionRate,
  };
}
