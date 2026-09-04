#!/usr/bin/env node

import { GENERATED_APP_LAYOUT_EXIT_CODE, resetBackend } from '../src/stacks/backend-reset.js';
import { GeneratedAppLayoutError } from '../src/runtime/spacetime-layout.js';
import { redactCredentials } from '../src/evidence/diagnostic-sanitizer.js';

const [backend, app] = process.argv.slice(2);
if (!backend || !app) throw new Error('usage: node dist/commands/reset-backend.js <backend> <app-dir>');

Promise.resolve().then(() => resetBackend({ backend, app })).then(result => {
  console.log(result);
}).catch(error => {
  if (error instanceof GeneratedAppLayoutError || error?.code === 'generated_app_layout') {
    console.error(`GENERATED_APP_LAYOUT: ${error.message}`);
    process.exitCode = GENERATED_APP_LAYOUT_EXIT_CODE;
    return;
  }
  const childOutput = [error?.stderr, error?.stdout]
    .filter(value => value !== undefined && value !== null && String(value).trim())
    .map(value => String(value).trim()).join('\n');
  if (childOutput) console.error(redactCredentials(childOutput).slice(-2000));
  console.error(error.stack ?? error.message);
  process.exitCode = 1;
});
