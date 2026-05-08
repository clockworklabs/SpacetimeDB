import hdr from 'hdr-histogram-js';
import { performance } from 'node:perf_hooks';
import { pickTwoDistinct, zipfSampler } from './zipf.ts';
import { makeCollisionTracker } from './collision_tracker.ts';
import { RunResult } from './types.ts';
import { BaseConnector } from './connectors.ts';
import type { RunnerRuntimeConfig } from '../config.ts';

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
  defaultTimeoutMs: number,
  timeoutOverrideMs?: number,
): Promise<T> {
  const timeoutMs = timeoutOverrideMs ?? defaultTimeoutMs;
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
  runtimeConfig,
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
  runtimeConfig: RunnerRuntimeConfig;
}): Promise<RunResult> {
  const {
    benchPipelined,
    logErrors,
    maxInflightPerWorker,
    minOpTimeoutMs,
    opTimeoutMs,
    precomputedTransferPairs,
    tailSlackMs,
    verifyTransactions,
  } = runtimeConfig;

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

  const precomputedPairs = precomputedTransferPairs;

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

  const PIPELINED = benchPipelined ?? !!connector.maxInflightPerWorker;
  const MAX_INFLIGHT_PER_WORKER =
    maxInflightPerWorker === undefined
      ? (connector.maxInflightPerWorker ?? 8)
      : maxInflightPerWorker == 0
        ? Infinity
        : maxInflightPerWorker;

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
            minOpTimeoutMs,
            Math.min(opTimeoutMs, timeLeft + tailSlackMs),
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
              opTimeoutMs,
              dynamicTimeout,
            );
            ok = true;
          } catch (err) {
            if (logErrors) {
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
              opTimeoutMs,
              dynamicTimeout,
            );
            const t1 = performance.now();
            completedTotal++;
            if (t1 <= endAt) {
              completedWithinWindow++;
              hist.recordValue(Math.max(1, Math.round((t1 - t0) * 1e3)));
            }
          } catch (err) {
            if (logErrors) {
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
          minOpTimeoutMs,
          Math.min(opTimeoutMs, timeLeft + tailSlackMs),
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
      `[${connector.name}] Test window ended at ${((testWindowEndTime - start) / 1000).toFixed(2)}s; waiting for in-flight operations...`,
    );

    // Now wait for all workers to fully complete (including in-flight ops)
    await Promise.all(workerPromises);

    return { start, completedWithinWindow, completedTotal };
  };

  console.log(`[${connector.name}] Starting workers for ${seconds}s run...`);

  const { start, completedWithinWindow, completedTotal } = await run(seconds);

  console.log(
    `[${connector.name}] All workers finished (including in-flight ops)`,
  );

  if (verifyTransactions) {
    console.log(`[${connector.name}] Running verification pass...`);
    try {
      await withOpTimeout(
        connector.verify(),
        `${connector.name} verify()`,
        opTimeoutMs,
      );
      console.log(`[${connector.name}] Verification passed`);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error(`[${connector.name}] Verification failed: ${msg}`);
    }
  }

  if (createWorker) {
    for (const w of workers) {
      const c = w as { close?: () => Promise<void> };
      if (typeof c.close === 'function') {
        try {
          await withOpTimeout(
            c.close(),
            `${connector.name} worker close`,
            opTimeoutMs,
          );
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

  await withOpTimeout(
    connector.close(),
    `${connector.name} root close`,
    opTimeoutMs,
  );

  const q = (p: number) => hist.getValueAtPercentile(p) / 1000;

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
    tps: completedWithinWindow / seconds,
    samples: completedWithinWindow,
    p50_ms: q(50),
    p95_ms: q(95),
    p99_ms: q(99),
    collision_ops: c.total,
    collision_count: c.collisions,
    collision_rate: c.collisionRate,
  };
}
