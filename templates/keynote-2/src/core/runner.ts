import hdr from 'hdr-histogram-js';
import { performance } from 'node:perf_hooks';
import { pickTwoDistinct, zipfSampler } from './zipf.ts';
import { getSpacetimeCommittedTransfers } from './spacetimeMetrics.ts';
import { makeCollisionTracker } from './collision_tracker.ts';
import { RunResult } from './types.ts';
import { BaseConnector } from './connectors.ts';

const OP_TIMEOUT_MS = Number(process.env.BENCH_OP_TIMEOUT_MS ?? '15000');
const MIN_OP_TIMEOUT_MS = Number(process.env.MIN_OP_TIMEOUT_MS ?? '250');
const TAIL_SLACK_MS = Number(process.env.TAIL_SLACK_MS ?? '1000');
const DEFAULT_PRECOMPUTED_TRANSFER_PAIRS = 10_000_000;

function precomputeZipfTransferPairs(
  accounts: number,
  alpha: number,
  count: number,
): { from: Uint32Array; to: Uint32Array; count: number } {
  const pick = zipfSampler(accounts, alpha);
  const from = new Uint32Array(count);
  const to = new Uint32Array(count);

  for (let i = 0; i < count; i++) {
    const [a, b] = pickTwoDistinct(pick);
    from[i] = a;
    to[i] = b;
  }

  return { from, to, count };
}

async function withOpTimeout<T>(
  promise: Promise<T>,
  label: string,
  timeoutOverrideMs?: number,
): Promise<T> {
  const timeoutMs = timeoutOverrideMs ?? OP_TIMEOUT_MS;
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
  connector: BaseConnector;
  scenario: (
    conn: BaseConnector,
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

  const { createWorker } = connector;

  const workers: BaseConnector[] = [];

  if (createWorker) {
    await connector.open(concurrency);

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

  const precomputedPairsRaw = Number(
    process.env.BENCH_PRECOMPUTED_TRANSFER_PAIRS ??
      DEFAULT_PRECOMPUTED_TRANSFER_PAIRS,
  );
  const precomputedPairs = Number.isFinite(precomputedPairsRaw)
    ? Math.max(1, Math.floor(precomputedPairsRaw))
    : DEFAULT_PRECOMPUTED_TRANSFER_PAIRS;

  console.log(
    `[${connector.name}] precomputing ${precomputedPairs} Zipf transfer pairs...`,
  );
  const precomputeStart = performance.now();
  const transferPairs = precomputeZipfTransferPairs(
    accounts,
    alpha,
    precomputedPairs,
  );
  const precomputeElapsedMs = performance.now() - precomputeStart;
  console.log(
    `[${connector.name}] precomputed ${transferPairs.count} pairs in ${(precomputeElapsedMs / 1000).toFixed(2)}s`,
  );

  const getEnvTernary = (envVal: string | undefined) => {
    switch (envVal) {
      case '0':
        return false;
      case '1':
        return true;
      default:
        return null;
    }
  };

  const PIPELINED =
    getEnvTernary(process.env.BENCH_PIPELINED) ??
    !!connector.maxInflightPerWorker;
  const MAX_INFLIGHT_ENV = process.env.MAX_INFLIGHT_PER_WORKER;
  const MAX_INFLIGHT_PER_WORKER =
    MAX_INFLIGHT_ENV == null
      ? (connector.maxInflightPerWorker ?? 8)
      : MAX_INFLIGHT_ENV === '0'
        ? Infinity
        : Number(MAX_INFLIGHT_ENV);

  console.log(
    `[${connector.name}] max inflight per worker: ${MAX_INFLIGHT_PER_WORKER}`,
  );
  const run = async (seconds: number) => {
    const start = performance.now();
    const endAt = start + seconds * 1000;

    let completedWithinWindow = 0;
    let completedTotal = 0;

    // Track when workers reach end of test window (before waiting for in-flight ops)
    let workersReachedEnd = 0;
    let resolveTestWindowEnd: () => void;
    const testWindowEndPromise = new Promise<void>((resolve) => {
      resolveTestWindowEnd = resolve;
    });

    function signalWorkerReachedEnd() {
      workersReachedEnd++;
      if (workersReachedEnd >= concurrency) {
        resolveTestWindowEnd();
      }
    }

    async function worker(workerIndex: number) {
      const conn = workers[workerIndex];
      const pairsPerWorker = Math.max(
        1,
        Math.floor(transferPairs.count / concurrency),
      );
      let pairIndex = workerIndex * pairsPerWorker;

      const nextTransferPair = (): [number, number] => {
        if (pairIndex >= transferPairs.count) {
          pairIndex = 0;
        }

        const from = transferPairs.from[pairIndex]!;
        const to = transferPairs.to[pairIndex]!;
        pairIndex++;
        return [from, to];
      };

      // non-pipelined
      if (!PIPELINED) {
        while (true) {
          const now = performance.now();
          if (now >= endAt) break;

          const timeLeft = endAt - now;
          const dynamicTimeout = Math.max(
            MIN_OP_TIMEOUT_MS,
            Math.min(OP_TIMEOUT_MS, timeLeft + TAIL_SLACK_MS),
          );

          const [from, to] = nextTransferPair();

          collisionTracker.begin(from);
          collisionTracker.begin(to);

          const t0 = performance.now();
          let ok = false;
          try {
            await withOpTimeout(
              scenario(conn, from, to, 1),
              `${connector.name} scenario ${from}->${to}`,
              dynamicTimeout,
            );
            ok = true;
          } catch (err) {
            if (process.env.LOG_ERRORS === '1') {
              const msg =
                err instanceof Error
                  ? `${err.name}: ${err.message}`
                  : String(err);
              console.warn(
                `[${connector.name}] Scenario failed for ${from} -> ${to}: ${msg}`,
              );
            }
          } finally {
            collisionTracker.end(from);
            collisionTracker.end(to);
          }

          const t1 = performance.now();
          if (ok) {
            completedTotal++;
            if (t1 <= endAt) {
              completedWithinWindow++;
              hist.recordValue(Math.max(1, Math.round((t1 - t0) * 1e3)));
            }
          }
        }
        signalWorkerReachedEnd();
        return;
      }

      // pipelined
      const inflight = new Set<Promise<void>>();
      const unlimitedInflight = !Number.isFinite(MAX_INFLIGHT_PER_WORKER);

      const launchOp = (dynamicTimeout: number) => {
        const [from, to] = nextTransferPair();

        collisionTracker.begin(from);
        collisionTracker.begin(to);

        const p = (async () => {
          const t0 = performance.now();
          try {
            await withOpTimeout(
              scenario(conn, from, to, 1),
              `${connector.name} scenario ${from}->${to}`,
              dynamicTimeout,
            );
            const t1 = performance.now();
            completedTotal++;
            if (t1 <= endAt) {
              completedWithinWindow++;
              hist.recordValue(Math.max(1, Math.round((t1 - t0) * 1e3)));
            }
          } catch (err) {
            if (process.env.LOG_ERRORS === '1') {
              const msg =
                err instanceof Error
                  ? `${err instanceof Error ? err.message : String(err)}`
                  : String(err);
              console.warn(
                `[${connector.name}] Scenario failed for ${from} -> ${to}: ${msg}`,
              );
            }
          } finally {
            collisionTracker.end(from);
            collisionTracker.end(to);
          }
        })();

        inflight.add(p);
        p.finally(() => {
          inflight.delete(p);
        });
      };

      while (true) {
        const now = performance.now();
        if (now >= endAt) break;

        const timeLeft = endAt - now;
        const dynamicTimeout = Math.max(
          MIN_OP_TIMEOUT_MS,
          Math.min(OP_TIMEOUT_MS, timeLeft + TAIL_SLACK_MS),
        );

        if (unlimitedInflight || inflight.size < MAX_INFLIGHT_PER_WORKER) {
          launchOp(dynamicTimeout);
        } else {
          await new Promise((resolve) => setTimeout(resolve, 0));
        }
      }

      // Signal that this worker has reached end of test window
      signalWorkerReachedEnd();

      await Promise.all(inflight);
    }

    // Start all workers - they run in parallel
    const workerPromises = Array.from({ length: concurrency }, (_, i) =>
      worker(i),
    );

    // Wait for all workers to reach end of test window (before they wait for in-flight ops)
    await testWindowEndPromise;

    const testWindowEndTime = performance.now();
    console.log(
      `[${connector.name}] Test window ended at ${((testWindowEndTime - start) / 1000).toFixed(2)}s; capturing metrics...`,
    );

    // Capture metrics immediately when test window ends
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
            `[spacetimedb] metrics at test window end: committed transfer txns = ${afterTransfers.toString()} (delta = ${deltaBig.toString()})`,
          );
        } else {
          console.warn(
            '[spacetimedb] metrics at test window end missing or decreased; ignoring metrics delta',
          );
        }
      } catch (err) {
        console.warn(
          '[spacetimedb] failed to read metrics at test window end; ignoring metrics delta:',
          err,
        );
      }
    }

    // Now wait for all workers to fully complete (including in-flight ops)
    await Promise.all(workerPromises);

    return { start, completedWithinWindow, completedTotal, committedDelta };
  };

  const warmUpSeconds = 5;
  console.log(`[${connector.name}] Warming up for ${warmUpSeconds}s...`);
  await run(warmUpSeconds);
  console.log(`[${connector.name}] Finished warmup.`);

  console.log(`[${connector.name}] Starting workers for ${seconds}s run...`);

  const { start, completedWithinWindow, completedTotal, committedDelta } =
    await run(seconds);

  console.log(
    `[${connector.name}] All workers finished (including in-flight ops)`,
  );

  if (process.env.VERIFY === '1') {
    console.log(`[${connector.name}] Running verification pass...`);
    try {
      await withOpTimeout(connector.verify(), `${connector.name} verify()`);
    } catch (err) {
      console.error(`[${connector.name}] Verification failed:`, err);
    }
  }

  if (createWorker) {
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

  const committedOrCompleted = committedDelta ?? completedWithinWindow;

  const c = collisionTracker.stats();
  const elapsedSeconds = (performance.now() - start) / 1000;

  console.log(
    '[runOne]',
    'completed within window =',
    completedWithinWindow,
    'total completed =',
    completedTotal,
    'window =',
    seconds,
    's, actual elapsed =',
    elapsedSeconds,
    's',
  );

  return {
    tps: committedOrCompleted / seconds,
    samples: completedWithinWindow,
    committed_txns: committedDelta,
    p50_ms: q(50),
    p95_ms: q(95),
    p99_ms: q(99),
    collision_ops: c.total,
    collision_count: c.collisions,
    collision_rate: c.collisionRate,
  };
}
