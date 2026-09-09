import { spawn } from 'node:child_process';
import { chmodSync, existsSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { readCampaignState } from '../src/campaigns/campaign-scheduler.js';
import { forwardControllerSignals } from './controller.js';
import { prepareStateVolume } from './state-volume.js';

export function demoConfiguration(setup: string, source: NodeJS.ProcessEnv = process.env) {
  const prepared: NodeJS.ProcessEnv = {};
  for (const line of setup.split('\n').filter(Boolean)) {
    const separator = line.indexOf('=');
    if (separator < 1) throw new Error('Invalid setup environment');
    prepared[line.slice(0, separator)] = line.slice(separator + 1);
  }
  const digest = prepared.STACK_BENCH_CONTROLLER_IMAGE?.match(/^(?:.*@)?sha256:([a-f0-9]{64})$/)?.[1];
  if (!digest) throw new Error('Demo requires a resolved controller image digest');
  prepared.STACK_BENCH_RELEASE_DEPS_VOLUME = `stack-bench-release-deps-${digest.slice(0, 12)}`;
  const output = `campaigns/demo-${digest.slice(0, 12)}`;
  const compose = ['compose', '-f', join(STACK_BENCH_ROOT, 'appliance/docker-compose.yaml')];
  return { env: { ...source, ...prepared }, output,
    dashboard: [...compose, '--profile', 'dashboard', 'up', '-d', 'dashboard'],
    campaign: [...compose, 'run', '--rm', '-T', '--name', `stack-bench-demo-${digest.slice(0, 12)}`,
      'controller', 'campaign', 'trial', 'plans/demo.json', '--out', output],
    savedEnvironment: Object.entries(prepared)
      .map(([key, value]) => `${key}=${value ?? ''}`).join('\n') + '\n',
  };
}

async function docker(args: string[], env: NodeJS.ProcessEnv): Promise<void> {
  const child = spawn('docker', args, { env, stdio: 'inherit' });
  const stopForwarding = forwardControllerSignals(child);
  try {
    await new Promise<void>((resolve, reject) => {
      child.once('error', reject);
      child.once('exit', (code, signal) => code === 0 ? resolve()
        : reject(new Error(`Demo Docker command failed (${signal ?? code})`)));
    });
  } finally { stopForwarding(); }
}

export async function demoCommand(args: string[]): Promise<void> {
  if (args.length) throw new Error('demo accepts no arguments; set image references through the environment');
  const config = demoConfiguration(prepareStateVolume());
  const environmentPath = '/state/controller-home/demo.env';
  writeFileSync(environmentPath, config.savedEnvironment, { mode: 0o600 });
  chmodSync(environmentPath, 0o600);
  await docker(config.dashboard, config.env);
  console.log('Stack Bench dashboard: http://127.0.0.1:7331');
  const directory = join('/state/results', config.output);
  if (existsSync(join(directory, 'state.json'))) {
    const { state } = readCampaignState(directory);
    console.log(`Existing demo campaign: ${state.status} (${config.output})`);
    if (state.status !== 'completed') throw new Error(`Existing demo needs attention: ${state.status}`);
    return;
  }
  await docker(config.campaign, config.env);
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) {
  demoCommand(process.argv.slice(2)).catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
