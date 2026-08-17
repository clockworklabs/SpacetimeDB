#!/usr/bin/env node

import { spawn } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));

const COMMANDS = Object.freeze({
  'init-deps': [join(ROOT, 'appliance', 'dependency-volume.mjs'), 'init'],
  'verify-deps': [join(ROOT, 'appliance', 'dependency-volume.mjs'), 'verify'],
  'preflight': [join(ROOT, 'preflight.mjs')],
  'qualify-reference': [join(ROOT, 'reference-live.mjs')],
  'qualify-null': [join(ROOT, 'null-control.mjs')],
  'qualification': [join(ROOT, 'qualification-cli.mjs')],
  'pack-budget': [join(ROOT, 'pack-budget.mjs')],
  'campaign': [join(ROOT, 'campaign-cli.mjs')],
  'repair': [join(ROOT, 'repair-cli.mjs')],
  'run': [join(ROOT, 'bench.mjs')],
  'verify-release': [join(ROOT, 'release-manifest.mjs'), 'verify'],
  'recover': [join(ROOT, 'recovery.mjs'), 'recover'],
  'test': ['--test', join(ROOT, 'tests', '*.test.mjs')],
});

export function resolveControllerCommand(argv) {
  const [command, ...rest] = argv;
  if (!command || command === '--help' || command === 'help') return null;
  if (!Object.hasOwn(COMMANDS, command)) throw new Error(`unknown controller command ${JSON.stringify(command)}`);
  return { executable: process.execPath, args: [...COMMANDS[command], ...rest] };
}

export function controllerChildEnvironment(source = process.env) {
  const mode = source.STACK_BENCH_AGENT_AUTH ?? 'credentials';
  if (!['credentials', 'subscription-token', 'api-key'].includes(mode)) {
    throw new Error('STACK_BENCH_AGENT_AUTH must be credentials, subscription-token, or api-key');
  }
  const env = { ...source };
  delete env.ANTHROPIC_API_KEY;
  delete env.ANTHROPIC_API_KEY_FILE;
  delete env.CLAUDE_CODE_OAUTH_TOKEN;
  delete env.CLAUDE_CODE_OAUTH_TOKEN_FILE;
  if (mode === 'api-key') {
    const path = source.STACK_BENCH_ANTHROPIC_API_KEY_FILE?.trim();
    if (!path) throw new Error('api-key auth requires STACK_BENCH_ANTHROPIC_API_KEY_FILE');
    env.ANTHROPIC_API_KEY_FILE = path;
  } else if (mode === 'subscription-token') {
    const path = source.STACK_BENCH_CLAUDE_OAUTH_TOKEN_FILE?.trim();
    if (!path) {
      throw new Error('subscription-token auth requires STACK_BENCH_CLAUDE_OAUTH_TOKEN_FILE');
    }
    env.CLAUDE_CODE_OAUTH_TOKEN_FILE = path;
  }
  return env;
}

function help() {
  process.stdout.write('Stack Bench controller\n\n'
    + 'Commands:\n'
    + '  preflight <exact run options>  verify the runner without a model call\n'
    + '  qualify-reference <scope>      repeat a pristine or mutation reference gate\n'
    + '  qualify-null <scope>           run the exact null-oracle gate\n'
    + '  qualification status <scope>  show exact launch and promotion blockers\n'
    + '  pack-budget recommend <scope> derive reviewable bounds from exact reference evidence\n'
    + '  campaign validate|show <file>  compile the exact comparison plan without running it\n'
    + '  campaign prepare|run <file> --out <dir>  checkpoint or execute a frozen plan\n'
    + '  campaign reconcile <file> --out <dir>  prove cleanup for interrupted work\n'
    + '  campaign status <dir>         inspect exact durable campaign state\n'
    + '  campaign report <dir>         regenerate deterministic JSON and static HTML\n'
    + '  repair status <run> --level N inspect whether a failed level can continue\n'
    + '  repair grant <run> --level N --rounds N  add one finite repair budget\n'
    + '  run <exact run options>        execute and retain one requested run\n'
    + '  verify-release <manifest>      verify candidate files or a qualified signed release\n'
    + '  recover <private-state>        retry authenticated cleanup or retain quarantine\n'
    + '  init-deps | verify-deps        initialize or verify the release dependency volume\n'
    + '  test                           run the model-free harness test suite\n');
}

async function main(argv) {
  const resolved = resolveControllerCommand(argv.slice(2));
  if (!resolved) { help(); return; }
  const child = spawn(resolved.executable, resolved.args,
    { stdio: 'inherit', env: controllerChildEnvironment(process.env) });
  for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, () => child.kill(signal));
  const outcome = await new Promise((resolveExit, reject) => {
    child.once('error', reject);
    child.once('exit', (code, signal) => resolveExit({ code, signal }));
  });
  if (outcome.signal) process.kill(process.pid, outcome.signal);
  process.exitCode = outcome.code ?? 1;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main(process.argv).catch(error => {
    console.error(`stack-bench-controller: ${error.message}`);
    process.exitCode = 2;
  });
}
