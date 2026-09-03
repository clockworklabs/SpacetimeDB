#!/usr/bin/env node

import { spawn } from 'node:child_process';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { STACK_BENCH_ROOT } from '../src/package-root.js';

const RUNTIME_ROOT = join(STACK_BENCH_ROOT, 'dist');

const COMMANDS = Object.freeze({
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

const COMMANDS_REQUIRING_AGENT_AUTH = new Set(['preflight', 'dashboard', 'run']);

export function controllerCommandRequiresAgentAuth(command: string | undefined,
  args: string[] = []): boolean {
  if (command && COMMANDS_REQUIRING_AGENT_AUTH.has(command)) return true;
  return command === 'campaign' && args[0] === 'run';
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
  process.stdout.write('Stack Bench controller\n\n'
    + 'Commands:\n'
    + '  preflight <exact run options>  verify the runner without a model call\n'
    + '  qualify-reference <scope>      run a pristine or mutation reference gate\n'
    + '    --mutation-workers N         split one mutation gate across 1 to 8 isolated workers\n'
    + '  qualify-null <scope>           run the exact null-oracle gate\n'
    + '  qualification status <scope>  show exact launch and promotion blockers\n'
    + '  pack-budget recommend <scope> derive reviewable bounds from exact reference evidence\n'
    + '  campaign validate|show <file>  compile the exact comparison plan without running it\n'
    + '  campaign trial <file> --out <dir>  exercise a model-free draft\n'
    + '  campaign run <file> --out <dir>  start a campaign\n'
    + '  campaign reconcile <file> --out <dir>  prove cleanup for interrupted work\n'
    + '  campaign status <dir>         inspect exact durable campaign state\n'
    + '  campaign report <dir>         regenerate deterministic JSON and static HTML\n'
    + '  dashboard [--port N]          serve the local operator dashboard\n'
    + '  repair status <run> --level N inspect whether a failed level can continue\n'
    + '  repair grant <run> --level N --repairs N  add one finite repair budget\n'
    + '  run <exact run options>        execute and retain one requested run\n'
    + '  verify-release <manifest>      verify candidate files or a qualified signed release\n'
    + '  recover <private-state>        retry authenticated cleanup or retain quarantine\n'
    + '  recover-lease <lease> --out <dir>  recover when parent state was not retained\n'
    + '  init-deps | verify-deps        initialize or verify the release dependency volume\n');
}

interface ChildOutcome {
  code: number | null;
  signal: NodeJS.Signals | null;
}

async function main(argv: string[]): Promise<void> {
  const command = argv[2];
  const resolved = resolveControllerCommand(argv.slice(2));
  if (!resolved) { help(); return; }
  const child = spawn(resolved.executable, resolved.args,
    { stdio: 'inherit', env: controllerChildEnvironment(process.env,
      { requireAgentAuth: controllerCommandRequiresAgentAuth(command, argv.slice(3)) }) });
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
