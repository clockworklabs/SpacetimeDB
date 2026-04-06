import 'dotenv/config';

import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { spacetimedb } from '../connectors/spacetimedb.ts';
import {
  getSharedRuntimeDefaults,
  parseStdbCompression,
  type SpacetimeConnectorConfig,
  type StdbCompression,
} from '../config.ts';
import { getSpacetimeCommittedTransfers } from '../core/spacetimeMetrics.ts';
import { normalizeStdbUrl } from '../core/stdbUrl.ts';
import {
  getBoolFlag,
  getNumberFlag,
  getStringFlag,
  getStringListFlag,
  parseArgs,
} from './args.ts';
import type {
  CoordinatorPhase,
  CoordinatorState,
  EpochResult,
  GeneratorLocalState,
  GeneratorSnapshot,
  ReadyRequest,
  RegisterRequest,
  StartEpochRequest,
  StartEpochResponse,
  StoppedRequest,
} from './protocol.ts';
import { isoNow, sleep, writeJsonFile } from './util.ts';

type GeneratorRecord = {
  id: string;
  hostname: string;
  desiredConnections: number;
  openedConnections: number;
  localState: GeneratorLocalState;
  activeEpoch: number | null;
};

type ActiveEpoch = {
  epoch: number;
  label: string | null;
  participantIds: string[];
  participantConnections: number;
  stopAcks: Set<string>;
};

function json(
  res: ServerResponse,
  status: number,
  body: unknown,
): void {
  const payload = `${JSON.stringify(body, null, 2)}\n`;
  res.writeHead(status, {
    'content-type': 'application/json; charset=utf-8',
    'content-length': Buffer.byteLength(payload),
  });
  res.end(payload);
}

async function readJsonBody<T>(req: IncomingMessage): Promise<T> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }

  const raw = Buffer.concat(chunks).toString('utf8').trim();
  if (!raw) {
    throw new Error('Request body is empty');
  }
  return JSON.parse(raw) as T;
}

async function runVerification(
  url: string,
  moduleName: string,
  compression: StdbCompression,
): Promise<void> {
  const prevVerify = process.env.VERIFY;
  process.env.VERIFY = '1';

  const defaults = getSharedRuntimeDefaults();
  const config: SpacetimeConnectorConfig = {
    initialBalance: defaults.initialBalance,
    stdbCompression: compression,
    stdbConfirmedReads: defaults.stdbConfirmedReads,
    stdbModule: moduleName,
    stdbUrl: url,
  };

  const conn = spacetimedb(config);
  try {
    await conn.open();
    await conn.verify();
  } finally {
    await conn.close();
    if (prevVerify == null) {
      delete process.env.VERIFY;
    } else {
      process.env.VERIFY = prevVerify;
    }
  }
}

class DistributedCoordinator {
  private readonly testName: string;
  private readonly connectorName: string;
  private readonly warmupMs: number;
  private readonly windowMs: number;
  private readonly verifyAfterEpoch: boolean;
  private readonly stopAckTimeoutMs: number;
  private readonly resultsDir: string;
  private readonly stdbUrl: string;
  private readonly moduleName: string;
  private readonly stdbCompression: StdbCompression;

  private readonly generators = new Map<string, GeneratorRecord>();
  private phase: CoordinatorPhase = 'idle';
  private currentEpoch: ActiveEpoch | null = null;
  private nextEpoch = 1;
  private lastResult: EpochResult | null = null;
  private epochTask: Promise<void> | null = null;

  constructor(opts: {
    testName: string;
    connectorName: string;
    warmupMs: number;
    windowMs: number;
    verifyAfterEpoch: boolean;
    stopAckTimeoutMs: number;
    resultsDir: string;
    stdbUrl: string;
    moduleName: string;
    stdbCompression: StdbCompression;
  }) {
    this.testName = opts.testName;
    this.connectorName = opts.connectorName;
    this.warmupMs = opts.warmupMs;
    this.windowMs = opts.windowMs;
    this.verifyAfterEpoch = opts.verifyAfterEpoch;
    this.stopAckTimeoutMs = opts.stopAckTimeoutMs;
    this.resultsDir = opts.resultsDir;
    this.stdbUrl = opts.stdbUrl;
    this.moduleName = opts.moduleName;
    this.stdbCompression = opts.stdbCompression;
  }

  snapshot(): CoordinatorState {
    const generators = Array.from(this.generators.values())
      .sort((a, b) => a.id.localeCompare(b.id))
      .map<GeneratorSnapshot>((generator) => ({
        id: generator.id,
        hostname: generator.hostname,
        desiredConnections: generator.desiredConnections,
        openedConnections: generator.openedConnections,
        localState: generator.localState,
        activeEpoch: generator.activeEpoch,
      }));

    return {
      phase: this.phase,
      currentEpoch: this.currentEpoch?.epoch ?? null,
      currentLabel: this.currentEpoch?.label ?? null,
      participants: this.currentEpoch?.participantIds ?? [],
      test: this.testName,
      connector: this.connectorName,
      generators,
      lastResult: this.lastResult,
    };
  }

  register(body: RegisterRequest): CoordinatorState {
    this.generators.set(body.id, {
      id: body.id,
      hostname: body.hostname,
      desiredConnections: body.desiredConnections,
      openedConnections: 0,
      localState: 'registered',
      activeEpoch: null,
    });
    return this.snapshot();
  }

  ready(body: ReadyRequest): CoordinatorState {
    const generator = this.requireGenerator(body.id);
    generator.openedConnections = body.openedConnections;
    generator.localState = 'ready';
    generator.activeEpoch = null;
    return this.snapshot();
  }

  stopped(body: StoppedRequest): CoordinatorState {
    const generator = this.requireGenerator(body.id);
    generator.localState = 'ready';
    generator.activeEpoch = null;

    if (this.currentEpoch && body.epoch === this.currentEpoch.epoch) {
      this.currentEpoch.stopAcks.add(body.id);
    }

    return this.snapshot();
  }

  startEpoch(body: StartEpochRequest): StartEpochResponse {
    if (this.phase !== 'idle' || this.currentEpoch || this.epochTask) {
      return {
        started: false,
        message: `Coordinator is busy in phase ${this.phase}`,
        state: this.snapshot(),
      };
    }

    const requestedIds = body.generatorIds?.length
      ? new Set(body.generatorIds)
      : null;

    const eligible = Array.from(this.generators.values()).filter((generator) => {
      if (requestedIds && !requestedIds.has(generator.id)) return false;
      return generator.localState === 'ready';
    });

    if (requestedIds) {
      const seen = new Set(eligible.map((generator) => generator.id));
      const missing = Array.from(requestedIds).filter((id) => !seen.has(id));
      if (missing.length > 0) {
        return {
          started: false,
          message: `Requested generators are not ready: ${missing.join(', ')}`,
          state: this.snapshot(),
        };
      }
    }

    if (eligible.length === 0) {
      return {
        started: false,
        message: 'No ready generators available for the next epoch',
        state: this.snapshot(),
      };
    }

    const activeEpoch: ActiveEpoch = {
      epoch: this.nextEpoch++,
      label: body.label ?? null,
      participantIds: eligible.map((generator) => generator.id).sort(),
      participantConnections: eligible.reduce(
        (sum, generator) => sum + generator.openedConnections,
        0,
      ),
      stopAcks: new Set<string>(),
    };

    for (const participantId of activeEpoch.participantIds) {
      const generator = this.generators.get(participantId);
      if (!generator) continue;
      generator.localState = 'running';
      generator.activeEpoch = activeEpoch.epoch;
    }

    this.currentEpoch = activeEpoch;
    this.phase = 'warmup';
    this.epochTask = this.runEpoch(activeEpoch)
      .catch((err) => {
        const msg = err instanceof Error ? err.message : String(err);
        console.error(`[coordinator] epoch ${activeEpoch.epoch} failed: ${msg}`);
      })
      .finally(() => {
        this.phase = 'idle';
        this.currentEpoch = null;
        this.epochTask = null;
      });

    console.log(
      `[coordinator] starting epoch ${activeEpoch.epoch} with ${activeEpoch.participantIds.length} generators and ${activeEpoch.participantConnections} total connections`,
    );

    return {
      started: true,
      message: `Epoch ${activeEpoch.epoch} started`,
      state: this.snapshot(),
    };
  }

  private async runEpoch(activeEpoch: ActiveEpoch): Promise<void> {
    let committedBefore = 0n;
    let committedAfter = 0n;
    let measuredAtMs = 0;
    let finishedAtMs = 0;
    let verification: EpochResult['verification'] = 'skipped';
    let verificationError: string | undefined;
    let error: string | undefined;

    try {
      console.log(
        `[coordinator] epoch ${activeEpoch.epoch} warmup for ${(this.warmupMs / 1000).toFixed(1)}s`,
      );
      await sleep(this.warmupMs);

      const before = await getSpacetimeCommittedTransfers(this.stdbUrl);
      if (before == null) {
        throw new Error(
          'Failed to read spacetime committed transfer counter at measurement start',
        );
      }
      committedBefore = before;
      measuredAtMs = Date.now();
      this.phase = 'measure';

      console.log(
        `[coordinator] epoch ${activeEpoch.epoch} measure start: committed=${committedBefore.toString()}`,
      );
      await sleep(this.windowMs);

      const after = await getSpacetimeCommittedTransfers(this.stdbUrl);
      if (after == null) {
        throw new Error(
          'Failed to read spacetime committed transfer counter at measurement end',
        );
      }
      committedAfter = after;
      finishedAtMs = Date.now();
      this.phase = 'stop';

      console.log(
        `[coordinator] epoch ${activeEpoch.epoch} measure end: committed=${committedAfter.toString()}`,
      );
      const pendingStops = await this.waitForStops(activeEpoch);
      if (pendingStops.length > 0) {
        error = `Missing stop acknowledgements from: ${pendingStops.join(', ')}`;
      }

      if (this.verifyAfterEpoch) {
        try {
          await runVerification(
            this.stdbUrl,
            this.moduleName,
            this.stdbCompression,
          );
          verification = 'passed';
        } catch (err) {
          verification = 'failed';
          verificationError = err instanceof Error ? err.message : String(err);
          console.error(
            `[coordinator] verification failed after epoch ${activeEpoch.epoch}: ${verificationError}`,
          );
        }
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      finishedAtMs = finishedAtMs || Date.now();
      this.phase = 'stop';
      console.error(`[coordinator] epoch ${activeEpoch.epoch} error: ${error}`);
      const pendingStops = await this.waitForStops(activeEpoch);
      if (pendingStops.length > 0) {
        error = `${error}; missing stop acknowledgements from: ${pendingStops.join(', ')}`;
      }
    }

    const actualWindowSeconds =
      measuredAtMs > 0 && finishedAtMs >= measuredAtMs
        ? (finishedAtMs - measuredAtMs) / 1000
        : this.windowMs / 1000;
    const committedDelta =
      committedAfter >= committedBefore
        ? committedAfter - committedBefore
        : 0n;
    const tps =
      actualWindowSeconds > 0
        ? Number(committedDelta) / actualWindowSeconds
        : 0;

    const result: EpochResult = {
      epoch: activeEpoch.epoch,
      label: activeEpoch.label,
      test: this.testName,
      connector: this.connectorName,
      warmupSeconds: this.warmupMs / 1000,
      windowSeconds: this.windowMs / 1000,
      actualWindowSeconds,
      participantIds: activeEpoch.participantIds,
      participantConnections: activeEpoch.participantConnections,
      measuredAt: measuredAtMs > 0 ? isoNow(measuredAtMs) : isoNow(),
      finishedAt: isoNow(finishedAtMs || Date.now()),
      committedBefore: committedBefore.toString(),
      committedAfter: committedAfter.toString(),
      committedDelta: committedDelta.toString(),
      tps,
      verification,
      verificationError,
      error,
    };

    this.lastResult = result;

    const fileName = `${this.testName}-${this.connectorName}-epoch-${String(
      result.epoch,
    ).padStart(3, '0')}-${result.finishedAt.replace(/[:.]/g, '-')}.json`;
    const outPath = join(this.resultsDir, fileName);
    await writeJsonFile(outPath, result);
    console.log(`[coordinator] wrote epoch ${result.epoch} result to ${outPath}`);
  }

  private async waitForStops(activeEpoch: ActiveEpoch): Promise<string[]> {
    const deadline = Date.now() + this.stopAckTimeoutMs;

    while (Date.now() < deadline) {
      if (activeEpoch.stopAcks.size >= activeEpoch.participantIds.length) {
        return [];
      }
      await sleep(250);
    }

    const pending = activeEpoch.participantIds.filter(
      (id) => !activeEpoch.stopAcks.has(id),
    );
    console.warn(
      `[coordinator] stop acknowledgements timed out for epoch ${activeEpoch.epoch}: ${pending.join(', ')}`,
    );
    return pending;
  }

  private requireGenerator(id: string): GeneratorRecord {
    const generator = this.generators.get(id);
    if (!generator) {
      throw new Error(`Unknown generator "${id}"`);
    }
    return generator;
  }
}

async function main(): Promise<void> {
  const { flags } = parseArgs(process.argv.slice(2));

  const bind = getStringFlag(flags, 'bind', '0.0.0.0');
  const port = getNumberFlag(flags, 'port', 8080);
  const testName = getStringFlag(flags, 'test', 'test-1');
  const connectorName = getStringFlag(flags, 'connector', 'spacetimedb');
  if (connectorName !== 'spacetimedb') {
    throw new Error(
      `Distributed coordinator currently supports only --connector spacetimedb, got ${connectorName}`,
    );
  }

  const defaultResultsDir = fileURLToPath(
    new URL('../../runs/distributed/', import.meta.url),
  );
  const resultsDir = getStringFlag(flags, 'results-dir', defaultResultsDir);
  const warmupSeconds = getNumberFlag(flags, 'warmup-seconds', 15);
  const windowSeconds = getNumberFlag(flags, 'window-seconds', 60);
  const stopAckTimeoutSeconds = getNumberFlag(
    flags,
    'stop-ack-timeout-seconds',
    60,
  );
  const verifyAfterEpoch = getBoolFlag(flags, 'verify', false);
  const rawStdbUrl = getStringFlag(
    flags,
    'stdb-url',
    process.env.STDB_URL ?? 'ws://127.0.0.1:3000',
  );
  const stdbUrl = normalizeStdbUrl(rawStdbUrl);
  const defaults = getSharedRuntimeDefaults();
  const moduleName = getStringFlag(
    flags,
    'stdb-module',
    process.env.STDB_MODULE ?? 'test-1',
  );
  const stdbCompression = parseStdbCompression(
    getStringFlag(flags, 'stdb-compression', defaults.stdbCompression),
    '--stdb-compression',
  );
  const initialIds = getStringListFlag(flags, 'generator-ids');

  const coordinator = new DistributedCoordinator({
    testName,
    connectorName,
    warmupMs: warmupSeconds * 1000,
    windowMs: windowSeconds * 1000,
    verifyAfterEpoch,
    stopAckTimeoutMs: stopAckTimeoutSeconds * 1000,
    resultsDir,
    stdbUrl,
    moduleName,
    stdbCompression,
  });

  const server = createServer(async (req, res) => {
    try {
      const method = req.method ?? 'GET';
      const url = new URL(req.url ?? '/', `http://${req.headers.host ?? 'localhost'}`);
      const path = url.pathname;

      if (method === 'GET' && path === '/healthz') {
        json(res, 200, { ok: true });
        return;
      }

      if (method === 'GET' && (path === '/state' || path === '/admin/status')) {
        json(res, 200, coordinator.snapshot());
        return;
      }

      if (method === 'POST' && path === '/register') {
        const body = await readJsonBody<RegisterRequest>(req);
        json(res, 200, coordinator.register(body));
        return;
      }

      if (method === 'POST' && path === '/ready') {
        const body = await readJsonBody<ReadyRequest>(req);
        json(res, 200, coordinator.ready(body));
        return;
      }

      if (method === 'POST' && path === '/stopped') {
        const body = await readJsonBody<StoppedRequest>(req);
        json(res, 200, coordinator.stopped(body));
        return;
      }

      if (method === 'POST' && path === '/admin/start-epoch') {
        const body =
          req.headers['content-length'] == null ||
          req.headers['content-length'] === '0'
            ? ({ generatorIds: initialIds } satisfies StartEpochRequest)
            : await readJsonBody<StartEpochRequest>(req);
        const merged: StartEpochRequest = {
          label: body.label,
          generatorIds: body.generatorIds ?? initialIds,
        };
        json(res, 200, coordinator.startEpoch(merged) satisfies StartEpochResponse);
        return;
      }

      json(res, 404, { error: `Unknown route: ${method} ${path}` });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      json(res, 400, { error: msg });
    }
  });

  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, bind, () => resolve());
  });

  console.log(
    `[coordinator] listening on http://${bind}:${port} test=${testName} connector=${connectorName} warmup=${warmupSeconds}s window=${windowSeconds}s verify=${verifyAfterEpoch ? 'on' : 'off'} stdb=${stdbUrl} compression=${stdbCompression}`,
  );
}

main().catch((err) => {
  const msg = err instanceof Error ? err.stack ?? err.message : String(err);
  console.error(msg);
  process.exitCode = 1;
});
