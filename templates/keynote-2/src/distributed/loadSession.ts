import type { ReducerConnector } from '../core/connectors.ts';
import { performance } from 'node:perf_hooks';
import { pickTwoDistinct, zipfSampler } from '../core/zipf.ts';
import type { DistributedLoadOptions } from './protocol.ts';

const OP_TIMEOUT_MS = Number(process.env.BENCH_OP_TIMEOUT_MS ?? '15000');
const DEFAULT_PRECOMPUTED_TRANSFER_PAIRS = 10_000_000;

type TransferPairs = {
  from: Uint32Array;
  to: Uint32Array;
  count: number;
};

function precomputeZipfTransferPairs(
  accounts: number,
  alpha: number,
  count: number,
): TransferPairs {
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
  timeoutMs = OP_TIMEOUT_MS,
): Promise<T> {
  let timer: NodeJS.Timeout | undefined;

  const timeoutPromise = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      reject(new Error(`${label} timed out after ${timeoutMs}ms`));
    }, timeoutMs);
  });

  try {
    return (await Promise.race([promise, timeoutPromise])) as T;
  } finally {
    if (timer) clearTimeout(timer);
  }
}

type LoadSessionConfig = {
  makeConnector: () => ReducerConnector;
  scenario: (
    conn: ReducerConnector,
    from: number,
    to: number,
    amount: number,
  ) => Promise<void>;
  concurrency: number;
  accounts: number;
  alpha: number;
  openParallelism: number;
};

type RunState = {
  epoch: number;
  stopRequested: boolean;
  workerPromises: Promise<void>[];
  loadOptions: DistributedLoadOptions;
};

export class LoadSession {
  private readonly makeConnector: () => ReducerConnector;
  private readonly scenario: LoadSessionConfig['scenario'];
  private readonly concurrency: number;
  private readonly accounts: number;
  private readonly alpha: number;
  private readonly openParallelism: number;

  private readonly pairs: TransferPairs;
  private readonly conns: Array<ReducerConnector | undefined>;
  private runState: RunState | null = null;

  constructor(config: LoadSessionConfig) {
    this.makeConnector = config.makeConnector;
    this.scenario = config.scenario;
    this.concurrency = config.concurrency;
    this.accounts = config.accounts;
    this.alpha = config.alpha;
    this.openParallelism = Math.max(1, Math.floor(config.openParallelism));

    const precomputedPairsRaw = Number(
      process.env.BENCH_PRECOMPUTED_TRANSFER_PAIRS ??
        DEFAULT_PRECOMPUTED_TRANSFER_PAIRS,
    );
    const precomputedPairs = Number.isFinite(precomputedPairsRaw)
      ? Math.max(1, Math.floor(precomputedPairsRaw))
      : DEFAULT_PRECOMPUTED_TRANSFER_PAIRS;

    console.log(
      `[distributed] precomputing ${precomputedPairs} Zipf transfer pairs...`,
    );
    const precomputeStart = performance.now();
    this.pairs = precomputeZipfTransferPairs(
      this.accounts,
      this.alpha,
      precomputedPairs,
    );
    const elapsedMs = performance.now() - precomputeStart;
    console.log(
      `[distributed] precomputed ${this.pairs.count} pairs in ${(elapsedMs / 1000).toFixed(2)}s`,
    );

    this.conns = new Array<ReducerConnector | undefined>(this.concurrency);
  }

  get openedConnections(): number {
    return this.conns.filter(Boolean).length;
  }

  async open(): Promise<void> {
    if (this.openedConnections === this.concurrency) return;

    const lanes = Math.min(this.openParallelism, this.concurrency);
    console.log(
      `[distributed] opening ${this.concurrency} connections with parallelism ${lanes}...`,
    );

    try {
      await Promise.all(
        Array.from({ length: lanes }, (_, lane) => this.openLane(lane, lanes)),
      );
      console.log(`[distributed] opened ${this.openedConnections} connections`);
    } catch (err) {
      await this.close();
      throw err;
    }
  }

  async startEpoch(epoch: number, loadOptions: DistributedLoadOptions): Promise<void> {
    if (this.openedConnections !== this.concurrency) {
      throw new Error(
        `Cannot start epoch ${epoch}: expected ${this.concurrency} open connections, got ${this.openedConnections}`,
      );
    }
    if (this.runState) {
      throw new Error(
        `Cannot start epoch ${epoch}: already running epoch ${this.runState.epoch}`,
      );
    }

    const runState: RunState = {
      epoch,
      stopRequested: false,
      workerPromises: [],
      loadOptions,
    };
    this.runState = runState;

    const mode = loadOptions.pipelined
      ? `pipelined max_inflight=${loadOptions.maxInflightPerConnection}`
      : 'closed-loop';
    console.log(`[distributed] starting epoch ${epoch} in ${mode} mode`);

    runState.workerPromises = this.conns.map((conn, workerIndex) => {
      if (!conn) {
        throw new Error(`Connection ${workerIndex} not open`);
      }
      return this.workerLoop(conn, workerIndex, runState);
    });
  }

  async stopEpoch(): Promise<void> {
    if (!this.runState) return;

    const runState = this.runState;
    runState.stopRequested = true;
    await Promise.all(runState.workerPromises);

    if (this.runState === runState) {
      this.runState = null;
    }
  }

  async close(): Promise<void> {
    await this.stopEpoch();

    const lanes = Math.min(this.openParallelism, this.concurrency);
    await Promise.all(
      Array.from({ length: lanes }, (_, lane) => this.closeLane(lane, lanes)),
    );
  }

  private async openLane(lane: number, lanes: number): Promise<void> {
    for (let index = lane; index < this.concurrency; index += lanes) {
      const conn = this.makeConnector();
      await conn.open();
      this.conns[index] = conn;
    }
  }

  private async closeLane(lane: number, lanes: number): Promise<void> {
    for (let index = lane; index < this.concurrency; index += lanes) {
      const conn = this.conns[index];
      if (!conn) continue;
      try {
        await conn.close();
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        console.warn(`[distributed] close failed for connection ${index}: ${msg}`);
      } finally {
        this.conns[index] = undefined;
      }
    }
  }

  private async workerLoop(
    conn: ReducerConnector,
    workerIndex: number,
    runState: RunState,
  ): Promise<void> {
    const nextTransferPair = this.makeTransferPairPicker(workerIndex);
    if (!runState.loadOptions.pipelined) {
      await this.closedLoopWorker(conn, workerIndex, runState, nextTransferPair);
      return;
    }

    await this.pipelinedWorker(conn, workerIndex, runState, nextTransferPair);
  }

  private makeTransferPairPicker(workerIndex: number): () => [number, number] {
    const pairsPerWorker = Math.max(
      1,
      Math.floor(this.pairs.count / this.concurrency),
    );
    let pairIndex = workerIndex * pairsPerWorker;

    return (): [number, number] => {
      if (pairIndex >= this.pairs.count) {
        pairIndex = 0;
      }

      const from = this.pairs.from[pairIndex]!;
      const to = this.pairs.to[pairIndex]!;
      pairIndex++;
      return [from, to];
    };
  }

  private async closedLoopWorker(
    conn: ReducerConnector,
    workerIndex: number,
    runState: RunState,
    nextTransferPair: () => [number, number],
  ): Promise<void> {
    while (!runState.stopRequested) {
      await this.runTransfer(conn, workerIndex, nextTransferPair);
    }
  }

  private async pipelinedWorker(
    conn: ReducerConnector,
    workerIndex: number,
    runState: RunState,
    nextTransferPair: () => [number, number],
  ): Promise<void> {
    const maxInflight = runState.loadOptions.maxInflightPerConnection;
    const inflight = new Set<Promise<void>>();

    const launchTransfer = () => {
      const transfer = this.runTransfer(conn, workerIndex, nextTransferPair);
      inflight.add(transfer);
      transfer.finally(() => {
        inflight.delete(transfer);
      });
    };

    while (!runState.stopRequested) {
      if (inflight.size < maxInflight) {
        launchTransfer();
      } else {
        await Promise.race(inflight);
      }
    }

    await Promise.all(inflight);
  }

  private async runTransfer(
    conn: ReducerConnector,
    workerIndex: number,
    nextTransferPair: () => [number, number],
  ): Promise<void> {
    const [from, to] = nextTransferPair();

    try {
      await withOpTimeout(
        this.scenario(conn, from, to, 1),
        `[distributed] worker ${workerIndex} transfer ${from}->${to}`,
      );
    } catch (err) {
      if (process.env.LOG_ERRORS === '1') {
        const msg = err instanceof Error ? err.message : String(err);
        console.warn(
          `[distributed] worker ${workerIndex} failed ${from}->${to}: ${msg}`,
        );
      }
    }
  }
}
