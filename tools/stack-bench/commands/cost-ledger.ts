#!/usr/bin/env node

import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { readRunJson } from '../src/evidence/artifacts.js';
import { durableCostLedger } from '../src/evidence/cost-proof.js';

export { durableCostLedger } from '../src/evidence/cost-proof.js';

function parseArgs(argv: string[]): string {
  const value = (name: string): string | null => {
    const index = argv.indexOf(name);
    return index < 0 ? null : argv[index + 1] ?? null;
  };
  const runPath = value('--run');
  const workdir = value('--workdir');
  if ((runPath === null) === (workdir === null)) {
    throw new Error('use exactly one of --run <run.json> or --workdir <run-directory>');
  }
  return runPath !== null ? resolve(runPath) : resolve(workdir as string, 'run.json');
}

const entrypoint = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : null;
if (import.meta.url === entrypoint) {
  try {
    const ledger = durableCostLedger(readRunJson(parseArgs(process.argv.slice(2))));
    process.stdout.write(`${JSON.stringify(ledger, null, 2)}\n`);
    if (!ledger.complete) process.exitCode = 1;
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 2;
  }
}
