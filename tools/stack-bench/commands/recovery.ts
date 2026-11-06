#!/usr/bin/env node

import { recoverBackendLease, recoverSupervisedRun } from '../src/runtime/recovery.js';

const [command, statePath, option, output] = process.argv.slice(2);
const supervisorRequest = command === 'recover' && statePath !== undefined && process.argv.length === 4;
const leaseRequest = command === 'recover-lease' && statePath !== undefined && option === '--out'
  && output !== undefined && process.argv.length === 6;
if (!supervisorRequest && !leaseRequest) {
  console.error('Usage:\n'
    + '  stack-bench recover <private-supervisor-state.json>\n'
    + '  stack-bench recover-lease <private-lease.json> --out <directory>');
  process.exit(2);
}

try {
  const result = leaseRequest
    ? recoverBackendLease(statePath, output)
    : recoverSupervisedRun(statePath);
  if (result === null) {
    console.log(`recovery: nothing to recover; supervisor state ${statePath} does not exist `
      + '(authenticated recovery removes it after proven cleanup)');
  } else console.log(JSON.stringify(result, null, 2));
  process.exitCode = result === null || result.ok ? 0 : 1;
} catch (error) {
  console.error(`recovery: ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 2;
}
