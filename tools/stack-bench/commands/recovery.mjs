#!/usr/bin/env node

import { recoverSupervisedRun } from '../src/runtime/recovery.mjs';

const [command, statePath] = process.argv.slice(2);
if (command !== 'recover' || !statePath || process.argv.length !== 4) {
  console.error('Usage: node commands/recovery.mjs recover <private-supervisor-state.json>');
  process.exit(2);
}

try {
  const result = recoverSupervisedRun(statePath);
  console.log(JSON.stringify(result, null, 2));
  process.exitCode = result.ok ? 0 : 1;
} catch (error) {
  console.error(`recovery: ${error.message}`);
  process.exitCode = 2;
}
