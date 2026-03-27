import 'dotenv/config';

import {
  getNumberFlag,
  getOptionalStringFlag,
  getStringFlag,
  getStringListFlag,
  parseArgs,
} from './args.ts';
import type {
  CoordinatorState,
  StartEpochResponse,
} from './protocol.ts';
import { getJson, postJson, sleep } from './util.ts';

function printState(state: CoordinatorState): void {
  console.log(
    `phase=${state.phase} epoch=${state.currentEpoch ?? '-'} label=${state.currentLabel ?? '-'}`,
  );
  console.log(
    `test=${state.test} connector=${state.connector} participants=${state.participants.length} mode=${state.loadOptions.pipelined ? `pipelined/${state.loadOptions.maxInflightPerConnection}` : 'closed-loop'}`,
  );

  if (state.generators.length === 0) {
    console.log('generators: none');
  } else {
    console.log('generators:');
    for (const generator of state.generators) {
      console.log(
        `  ${generator.id} host=${generator.hostname} state=${generator.localState} opened=${generator.openedConnections}/${generator.desiredConnections} active_epoch=${generator.activeEpoch ?? '-'}`,
      );
    }
  }

  if (state.lastResult) {
    console.log('last_result:');
    console.log(
      `  epoch=${state.lastResult.epoch} mode=${state.lastResult.loadOptions.pipelined ? `pipelined/${state.lastResult.loadOptions.maxInflightPerConnection}` : 'closed-loop'} tps=${state.lastResult.tps.toFixed(2)} delta=${state.lastResult.committedDelta} verification=${state.lastResult.verification}${state.lastResult.error ? ` error=${state.lastResult.error}` : ''}`,
    );
  }
}

async function waitForEpochResult(
  coordinatorUrl: string,
  epoch: number,
  pollMs: number,
): Promise<CoordinatorState> {
  for (;;) {
    const state = await getJson<CoordinatorState>(coordinatorUrl, '/admin/status');
    if (
      state.lastResult &&
      state.lastResult.epoch === epoch &&
      state.currentEpoch !== epoch
    ) {
      return state;
    }
    await sleep(pollMs);
  }
}

async function main(): Promise<void> {
  const { positionals, flags } = parseArgs(process.argv.slice(2));
  const command = positionals[0] ?? 'status';
  const coordinatorUrl = getStringFlag(
    flags,
    'coordinator-url',
    'http://127.0.0.1:8080',
  );

  switch (command) {
    case 'status': {
      const state = await getJson<CoordinatorState>(coordinatorUrl, '/admin/status');
      printState(state);
      return;
    }

    case 'start-epoch': {
      const label = getOptionalStringFlag(flags, 'label');
      const generatorIds = getStringListFlag(flags, 'generator-ids');
      const pollMs = getNumberFlag(flags, 'poll-ms', 1000);
      const response = await postJson<StartEpochResponse>(
        coordinatorUrl,
        '/admin/start-epoch',
        {
          label,
          generatorIds,
        },
      );
      console.log(response.message);
      printState(response.state);
      if (!response.started) {
        return;
      }

      const epoch = response.state.currentEpoch;
      if (epoch == null) {
        return;
      }

      console.log(`waiting for epoch ${epoch} to complete...`);
      const finalState = await waitForEpochResult(coordinatorUrl, epoch, pollMs);
      printState(finalState);
      return;
    }

    default:
      throw new Error(
        `Unknown command "${command}". Supported commands: status, start-epoch`,
      );
  }
}

main().catch((err) => {
  const msg = err instanceof Error ? err.stack ?? err.message : String(err);
  console.error(msg);
  process.exitCode = 1;
});
