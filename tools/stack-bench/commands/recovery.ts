#!/usr/bin/env node

import { recoverBackendLease, recoverSupervisedRun } from '../src/runtime/recovery.mjs';

const [command, statePath, option, output] = process.argv.slice(2);
const supervisorRequest = command === 'recover' && statePath !== undefined && process.argv.length === 4;
const leaseRequest = command === 'recover-lease' && statePath !== undefined && option === '--out'
  && output !== undefined && process.argv.length === 6;
if (!supervisorRequest && !leaseRequest) {
  console.error('Usage:\n'
    + '  stack-bench recover <private-supervisor-state.json>\n'
    + '  stack-bench recover-lease <private-backend-lease.json> --out <directory>');
  process.exit(2);
}

try {
  const result = leaseRequest
    ? recoverBackendLease(statePath, output)
    : recoverSupervisedRun(statePath);
  console.log(JSON.stringify(result, null, 2));
  process.exitCode = result.ok ? 0 : 1;
} catch (error) {
  console.error(`recovery: ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 2;
}
