#!/usr/bin/env node

import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { resolveContainerImage } from '../src/runtime/container-image.js';
import { parsePreflightArgs } from '../commands/preflight-cli.js';
import { parseBenchArguments } from '../commands/bench-arguments.js';
import { AGENT_ADAPTER_REGISTRY } from '../src/agents/agent-adapters.js';
import { stateVolumeCommand } from './state-volume.js';

const RUNTIME_ROOT = join(STACK_BENCH_ROOT, 'dist');

const COMMANDS = Object.freeze({
  'demo': [join(RUNTIME_ROOT, 'appliance', 'demo.js')],
  'init-deps': [join(RUNTIME_ROOT, 'appliance', 'dependency-volume.js'), 'init'],
  'verify-deps': [join(RUNTIME_ROOT, 'appliance', 'dependency-volume.js'), 'verify'],
  'preflight': [join(RUNTIME_ROOT, 'commands', 'preflight.js')],
  'qualify-reference': [join(RUNTIME_ROOT, 'src', 'references', 'reference-live.js')],
  'qualify-null': [join(RUNTIME_ROOT, 'commands', 'null-control.js')],
  'qualification': [join(RUNTIME_ROOT, 'commands', 'qualification-cli.js')],
  'pack-budget': [join(RUNTIME_ROOT, 'commands', 'pack-budget.js')],
  'campaign': [join(RUNTIME_ROOT, 'commands', 'campaign-cli.js')],
  'dashboard': [join(RUNTIME_ROOT, 'dashboard', 'dashboard-server.js')],
  'repair': [join(RUNTIME_ROOT, 'commands', 'repair-cli.js')],
  'run': [join(RUNTIME_ROOT, 'commands', 'bench.js')],
  'verify-release': [join(RUNTIME_ROOT, 'src', 'releases', 'release-manifest.js'), 'verify'],
  'recover': [join(RUNTIME_ROOT, 'commands', 'recovery.js'), 'recover'],
  'recover-lease': [join(RUNTIME_ROOT, 'commands', 'recovery.js'), 'recover-lease'],
} satisfies Record<string, readonly string[]>);

const COMMANDS_REQUIRING_AGENT_AUTH = new Set(['preflight', 'run']);

export function controllerCommandRequiresAgentAuth(command: string | undefined,
  args: string[] = []): boolean {
  if (command === 'run' && args.some(value => value === '--grade-from' || value.startsWith('--grade-from='))) {
    return !parseBenchArguments([process.execPath, 'bench', ...args]).gradeFrom;
  }
  if (command === 'preflight' && args.length) {
    const request = parsePreflightArgs([process.execPath, 'preflight', ...args]);
    return AGENT_ADAPTER_REGISTRY.get(request.agentAdapter).costLimit !== 'non-billable';
  }
  if (command && COMMANDS_REQUIRING_AGENT_AUTH.has(command)) return true;
  return command === 'campaign' && ['run', 'resume', 'extend'].includes(args[0] ?? '');
}

export function controllerRuntimeEnvironment(source: NodeJS.ProcessEnv = process.env,
  resolveImage = resolveContainerImage): NodeJS.ProcessEnv {
  if (!source.STACK_BENCH_CONTROLLER_IMAGE) {
    throw new Error('STACK_BENCH_CONTROLLER_IMAGE is required for runtime work');
  }
  return { ...source, STACK_BENCH_CONTROLLER_IMAGE_ID:
    resolveImage(source.STACK_BENCH_CONTROLLER_IMAGE).id };
}

export function controllerRuntimeCommand(args: string[], source: NodeJS.ProcessEnv = process.env) {
  if (!source.STACK_BENCH_COMPOSE_FILE || !source.STACK_BENCH_STATE_ROOT
    || !source.STACK_BENCH_CONTROLLER_IMAGE || !(source.STACK_BENCH_BUILD_IMAGE ?? source.STACK_BENCH_IMAGE)) {
    throw new Error('controller launch requires the setup environment and appliance Compose file');
  }
  const ownership = randomUUID();
  const containerName = `stack-bench-controller-${ownership}`;
  const ownershipLabel = `io.spacetimedb.stack-bench.controller-owner=${ownership}`;
  const env: NodeJS.ProcessEnv = { ...controllerChildEnvironment(source, { requireAgentAuth: false }),
    STACK_BENCH_BUILD_IMAGE: source.STACK_BENCH_BUILD_IMAGE ?? source.STACK_BENCH_IMAGE };
  return { executable: 'docker', containerName, ownershipLabel,
    args: ['compose', '-f', source.STACK_BENCH_COMPOSE_FILE, 'run', '--rm', '--no-deps',
      '--name', containerName, '--label', ownershipLabel, 'controller', ...args],
    env };
}

export interface ResolvedControllerCommand {
  executable: string;
  args: string[];
}

export function resolveControllerCommand(argv: string[]): ResolvedControllerCommand | null {
  const [command, ...rest] = argv;
  if (!command || command === '--help' || command === 'help') return null;
  if (!Object.hasOwn(COMMANDS, command)) {
    throw new Error(`unknown controller command ${JSON.stringify(command)}`);
  }
  return { executable: process.execPath,
    args: [...COMMANDS[command as keyof typeof COMMANDS], ...rest] };
}

export function controllerChildEnvironment(source: NodeJS.ProcessEnv = process.env,
  { requireAgentAuth = true }: { requireAgentAuth?: boolean } = {}): NodeJS.ProcessEnv {
  const env = { ...source };
  delete env.ANTHROPIC_API_KEY;
  delete env.ANTHROPIC_API_KEY_FILE;
  delete env.CLAUDE_CODE_OAUTH_TOKEN;
  delete env.CLAUDE_CODE_OAUTH_TOKEN_FILE;
  if (!requireAgentAuth) return env;
  const mode = source.STACK_BENCH_AGENT_AUTH ?? 'subscription-token';
  if (!['subscription-token', 'api-key'].includes(mode)) {
    throw new Error('STACK_BENCH_AGENT_AUTH must be subscription-token or api-key');
  }
  if (mode === 'api-key') {
    const path = source.STACK_BENCH_ANTHROPIC_API_KEY_FILE?.trim();
    if (!path) throw new Error('api-key auth requires STACK_BENCH_ANTHROPIC_API_KEY_FILE');
    env.ANTHROPIC_API_KEY_FILE = path;
  } else {
    const path = source.STACK_BENCH_CLAUDE_OAUTH_TOKEN_FILE?.trim();
    if (!path) {
      throw new Error('subscription-token auth requires STACK_BENCH_CLAUDE_OAUTH_TOKEN_FILE');
    }
    env.CLAUDE_CODE_OAUTH_TOKEN_FILE = path;
  }
  return env;
}

interface SignalChild {
  kill(signal: NodeJS.Signals): unknown;
}

interface SignalSource {
  on(signal: NodeJS.Signals, listener: () => void): unknown;
  off(signal: NodeJS.Signals, listener: () => void): unknown;
}

export function forwardControllerSignals(child: SignalChild,
  source: SignalSource = process): () => void {
  const signals: NodeJS.Signals[] = ['SIGINT', 'SIGTERM'];
  const listeners = new Map<NodeJS.Signals, () => void>(signals.map(signal =>
    [signal, () => { child.kill(signal); }]));
  for (const [signal, listener] of listeners) source.on(signal, listener);
  return () => {
    for (const [signal, listener] of listeners) source.off(signal, listener);
  };
}

function help(): void {
  process.stdout.write('Stack Bench controller\n'
    + '\n'
    + 'Docker setup\n'
    + '  setup                            prepare the state volume and print operator.env\n'
    + '  set-secret <name>                read one secret from stdin into the volume mounted at /state\n'
    + '\n'
    + 'A campaign compares stacks by building the same product on each. Point\n'
    + 'commands at the durable plans/ and campaigns/ directories.\n'
    + '\n'
    + 'Run a campaign\n'
    + '  preflight --backend <stacks> --track <track> --levels <range>\n'
    + '                                   verify the runner without creating an attempt\n'
    + '  campaign validate <plan>         compile a plan file and report what is wrong with it\n'
    + '  campaign show <plan>             print the compiled plan\n'
    + '  campaign trial <plan> --out <dir>  run the plan with a model-free agent\n'
    + '  campaign run <plan> --out <dir>  run the plan; run it again on the same <dir> to continue\n'
    + '  campaign resume <plan> --out <dir>  continue an interrupted dependency attempt from its saved state\n'
    + '  campaign extend <plan> --from <dir> --depth <n> --out <dir>  continue a finished campaign deeper\n'
    + '  campaign stop <dir>             stop owned active work and retain its evidence\n'
    + '  campaign status <dir> [--full]   what the campaign is doing now, from its saved state\n'
    + '  campaign inspect <dir>           every attempt, level, and check with its evidence\n'
    + '  campaign report <dir>            write the JSON and HTML report\n'
    + '  campaign audit <dir>             check a finished reference campaign against its promises\n'
    + '  campaign grant-repairs <dir> --attempt <id> --level <n> --repairs <n>  add repair budget\n'
    + '  campaign reconcile <plan> --out <dir>  clean up after an interruption and prove it\n'
    + '  campaign modes                   list the campaign modes this controller knows\n'
    + '  dashboard [--port N]             serve the local dashboard\n'
    + '\n'
    + 'One attempt outside a campaign\n'
    + '  run --backend <stack> --track <track> --levels <range> --out <dir> [...]  build and grade one attempt\n'
    + '  run --grade-from <run-dir> --grade-level <depth> --check <id> --out <fresh-dir>  replay saved dependency source without model calls\n'
    + '  repair status <run-dir> --level <n>  can a failed level continue?\n'
    + '  repair grant <run-dir> --level <n> --repairs <n>  add one repair budget\n'
    + '\n'
    + 'Qualify the grader\n'
    + '  qualify-reference --track <track> --level <n>  grade the hand-built reference app, or its mutations\n'
    + '    --mutation-workers N           split the mutation run across 1 to 8 isolated workers\n'
    + '  qualify-null --track <track> --level <n>  prove an empty app scores nothing\n'
    + '  qualification status --track <track> --level <n>  which grading evidence is still missing\n'
    + '  pack-budget recommend --track <track> --level <n> --recipe <id> --evidence <dir>  derive pack limits from reference evidence\n'
    + '\n'
    + 'Recover and verify\n'
    + '  recover <private-state>          retry cleanup for an interrupted attempt, or keep its quarantine\n'
    + '  recover-lease <lease> --out <dir>  recover when the attempt state was not kept\n'
    + '  verify-release <manifest>        verify a candidate or signed release\n'
    + '  init-deps | verify-deps          create or verify the release dependency volume\n');
}

interface ChildOutcome {
  code: number | null;
  signal: NodeJS.Signals | null;
}

async function main(argv: string[]): Promise<void> {
  const command = argv[2];
  if (command === 'setup' || command === 'set-secret') {
    stateVolumeCommand(command, argv.slice(3));
    return;
  }
  const resolved = resolveControllerCommand(argv.slice(2));
  if (!resolved) { help(); return; }
  let env = controllerChildEnvironment(process.env,
    { requireAgentAuth: controllerCommandRequiresAgentAuth(command, argv.slice(3)) });
  const runtime = ['preflight', 'run', 'qualify-reference', 'qualify-null', 'recover', 'recover-lease']
    .includes(command ?? '') || (command === 'campaign'
      && ['run', 'trial', 'resume', 'extend', 'reconcile'].includes(argv[3] ?? ''));
  if (runtime) env = controllerRuntimeEnvironment(env);
  const child = spawn(resolved.executable, resolved.args, { stdio: 'inherit', env });
  const stopForwardingSignals = forwardControllerSignals(child);
  let outcome: ChildOutcome;
  try {
    outcome = await new Promise<ChildOutcome>((resolveExit, reject) => {
      child.once('error', reject);
      child.once('exit', (code, signal) => { resolveExit({ code, signal }); });
    });
  } finally { stopForwardingSignals(); }
  if (outcome.signal) process.kill(process.pid, outcome.signal);
  process.exitCode = outcome.code ?? 1;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main(process.argv).catch((error: unknown) => {
    console.error(`stack-bench-controller: ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 2;
  });
}
