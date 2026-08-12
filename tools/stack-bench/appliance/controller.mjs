#!/usr/bin/env node

import { spawn } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));

const COMMANDS = Object.freeze({
  'init-deps': [join(ROOT, 'appliance', 'dependency-volume.mjs'), 'init'],
  'verify-deps': [join(ROOT, 'appliance', 'dependency-volume.mjs'), 'verify'],
  'preflight': [join(ROOT, 'preflight.mjs')],
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

function help() {
  process.stdout.write('Stack Bench controller\n\n'
    + 'Commands:\n'
    + '  preflight <exact run options>  verify the runner without a model call\n'
    + '  run <exact run options>        execute and retain one requested run\n'
    + '  verify-release <manifest>      verify candidate files or a qualified signed release\n'
    + '  recover <private-state>        retry authenticated cleanup or retain quarantine\n'
    + '  init-deps | verify-deps        initialize or verify the release dependency volume\n'
    + '  test                           run the model-free harness test suite\n');
}

async function main(argv) {
  const resolved = resolveControllerCommand(argv.slice(2));
  if (!resolved) { help(); return; }
  const child = spawn(resolved.executable, resolved.args, { stdio: 'inherit', env: process.env });
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
