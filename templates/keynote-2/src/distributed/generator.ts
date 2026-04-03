import 'dotenv/config';

import { hostname as getHostname } from 'node:os';
import { spacetimedb } from '../connectors/spacetimedb.ts';
import {
  getSharedRuntimeDefaults,
  parseStdbCompression,
  type SpacetimeConnectorConfig,
} from '../config.ts';
import type { ReducerConnector } from '../core/connectors.ts';
import { normalizeStdbUrl } from '../core/stdbUrl.ts';
import { getNumberFlag, getStringFlag, parseArgs } from './args.ts';
import { LoadSession } from './loadSession.ts';
import type { CoordinatorState } from './protocol.ts';
import { getJson, postJson, retryUntilSuccess, sleep } from './util.ts';
import { loadDistributedTestCase } from './testcase.ts';

async function main(): Promise<void> {
  const { flags } = parseArgs(process.argv.slice(2));

  const coordinatorUrl = getStringFlag(
    flags,
    'coordinator-url',
    'http://127.0.0.1:8080',
  );
  const testName = getStringFlag(flags, 'test', 'test-1');
  const connectorName = getStringFlag(flags, 'connector', 'spacetimedb');
  if (connectorName !== 'spacetimedb') {
    throw new Error(
      `Distributed generator currently supports only --connector spacetimedb, got ${connectorName}`,
    );
  }

  const id = getStringFlag(
    flags,
    'id',
    `${getHostname()}-${process.pid.toString(10)}`,
  );
  const hostname = getStringFlag(flags, 'hostname', getHostname());
  const concurrency = getNumberFlag(flags, 'concurrency');
  const accounts = getNumberFlag(
    flags,
    'accounts',
    process.env.SEED_ACCOUNTS ? Number(process.env.SEED_ACCOUNTS) : 100_000,
  );
  const alpha = getNumberFlag(flags, 'alpha', 1.5);
  const openParallelism = getNumberFlag(flags, 'open-parallelism', 128);
  const pollMs = getNumberFlag(flags, 'poll-ms', 1000);
  const controlRetries = getNumberFlag(flags, 'control-retries', 3);
  const rawStdbUrl = getStringFlag(
    flags,
    'stdb-url',
    process.env.STDB_URL ?? 'ws://127.0.0.1:3000',
  );
  const stdbUrl = normalizeStdbUrl(rawStdbUrl);
  const moduleName = getStringFlag(
    flags,
    'stdb-module',
    process.env.STDB_MODULE ?? 'test-1',
  );
  const defaults = getSharedRuntimeDefaults();
  const stdbCompression = parseStdbCompression(
    getStringFlag(flags, 'stdb-compression', defaults.stdbCompression),
    '--stdb-compression',
  );
  const connectorConfig: SpacetimeConnectorConfig = {
    initialBalance: defaults.initialBalance,
    stdbCompression,
    stdbConfirmedReads: defaults.stdbConfirmedReads,
    stdbModule: moduleName,
    stdbUrl,
  };

  const testCase = await loadDistributedTestCase(testName, 'spacetimedb');
  const session = new LoadSession({
    makeConnector: () => spacetimedb(connectorConfig),
    scenario: testCase.run as (
      conn: ReducerConnector,
      from: number,
      to: number,
      amount: number,
    ) => Promise<void>,
    concurrency,
    accounts,
    alpha,
    openParallelism,
  });

  let activeEpoch: number | null = null;
  let stopping = false;

  const stopActiveEpoch = async () => {
    if (activeEpoch == null) return;

    const epoch = activeEpoch;
    activeEpoch = null;
    console.log(`[generator ${id}] stopping epoch ${epoch}`);
    await session.stopEpoch();
    await retryUntilSuccess('[generator] stopped', async () => {
      await postJson<CoordinatorState>(coordinatorUrl, '/stopped', {
        id,
        epoch,
      });
    }, pollMs, controlRetries);
  };

  for (const sig of ['SIGINT', 'SIGTERM'] as const) {
    process.on(sig, () => {
      stopping = true;
    });
  }

  try {
    await retryUntilSuccess('[generator] register', async () => {
      await postJson<CoordinatorState>(coordinatorUrl, '/register', {
        id,
        hostname,
        desiredConnections: concurrency,
      });
    }, pollMs, controlRetries, () => !stopping);

    await session.open();

    await retryUntilSuccess('[generator] ready', async () => {
      await postJson<CoordinatorState>(coordinatorUrl, '/ready', {
        id,
        openedConnections: session.openedConnections,
      });
    }, pollMs, controlRetries, () => !stopping);

    console.log(
      `[generator ${id}] ready with ${session.openedConnections} connections to ${stdbUrl}/${moduleName} compression=${stdbCompression}`,
    );

    while (!stopping) {
      const state = await retryUntilSuccess(
        '[generator] fetch state',
        async () => await getJson<CoordinatorState>(coordinatorUrl, '/state'),
        pollMs,
        controlRetries,
        () => !stopping,
      );

      const isParticipant =
        activeEpoch != null &&
        state.currentEpoch === activeEpoch &&
        state.participants.includes(id);
      const shouldKeepRunning =
        isParticipant &&
        (state.phase === 'warmup' || state.phase === 'measure');

      if (!activeEpoch) {
        if (
          state.phase === 'warmup' &&
          state.currentEpoch != null &&
          state.participants.includes(id)
        ) {
          console.log(`[generator ${id}] starting epoch ${state.currentEpoch}`);
          await session.startEpoch(state.currentEpoch);
          activeEpoch = state.currentEpoch;
        }
      } else if (!shouldKeepRunning) {
        await stopActiveEpoch();
      }

      await sleep(pollMs);
    }
  } catch (err) {
    if (!stopping) {
      throw err;
    }
  } finally {
    await stopActiveEpoch();
    await session.close();
  }
}

main().catch((err) => {
  const msg = err instanceof Error ? err.stack ?? err.message : String(err);
  console.error(msg);
  process.exitCode = 1;
});
