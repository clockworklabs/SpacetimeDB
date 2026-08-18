#!/usr/bin/env node
// Lease-authenticated database reset used by the grading orchestrator.

import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { leaseFromEnv } from './backend-lease.mjs';
import { executeStackCapability } from './stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.mjs';
import { GeneratedAppLayoutError } from './spacetime-layout.mjs';

export const GENERATED_APP_LAYOUT_EXIT_CODE = 10;

export function resetBackend({ backend, app, exec }) {
  const { lease } = leaseFromEnv(process.env, { backend, active: true });
  return executeStackCapability(STACK_ADAPTER_REGISTRY.get(backend), 'reset', 'run',
    { lease, app, ...(exec ? { exec } : {}) });
}

async function main() {
  const [backend, app] = process.argv.slice(2);
  if (!backend || !app) throw new Error('usage: node reset-backend.mjs <backend> <app-dir>');
  console.log(resetBackend({ backend, app }));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => {
    if (error instanceof GeneratedAppLayoutError || error?.code === 'generated_app_layout') {
      console.error(`GENERATED_APP_LAYOUT: ${error.message}`);
      process.exitCode = GENERATED_APP_LAYOUT_EXIT_CODE;
      return;
    }
    console.error(error.stack ?? error.message);
    process.exitCode = 1;
  });
}
