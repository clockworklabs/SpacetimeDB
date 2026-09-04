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
  process.stdout.write('Stack Bench controller\n'
    + '\n'
    + 'A campaign compares stacks by building the same product on each. Point\n'
    + 'every command at the state root /var/lib/stack-bench.\n'
    + '\n'
    + 'Run a campaign\n'
    + '  preflight --backend <stacks> --track <track> --levels <range> [--smoke]\n'
    + '                                   verify the runner; --smoke starts a real coding container, no model\n'
    + '  campaign validate <plan>         compile a plan file and report what is wrong with it\n'
    + '  campaign show <plan>             print the compiled plan\n'
    + '  campaign trial <plan> --out <dir>  run the plan with a model-free agent\n'
    + '  campaign run <plan> --out <dir>  run the plan; run it again on the same <dir> to continue\n'
    + '  campaign resume <plan> --out <dir>  continue an interrupted dependency attempt from its saved state\n'
    + '  campaign extend <plan> --from <dir> --depth <n> --out <dir>  continue a finished campaign deeper\n'
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
    + '  repair status <run-dir> --level <n>  can a failed level continue?\n'
    + '  repair grant <run-dir> --level <n> --repairs <n>  add one repair budget\n'
    + '\n'
    + 'Qualify the grader\n'
    + '  qualify-reference --track <track> --level <n>  grade the hand-built reference app, or its mutations\n'
    + '    --mutation-workers N           split the mutation run across 1 to 8 isolated workers\n'
    + '  qualify-null --track <track> --level <n>  prove an empty app scores nothing\n'
    + '  qualification status --track <track> --level <n>  what still blocks a paid launch\n'
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
