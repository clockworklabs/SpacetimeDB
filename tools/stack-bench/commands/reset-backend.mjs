#!/usr/bin/env node

import { GENERATED_APP_LAYOUT_EXIT_CODE, resetBackend } from '../src/stacks/backend-reset.mjs';
import { GeneratedAppLayoutError } from '../src/runtime/spacetime-layout.mjs';

const [backend, app] = process.argv.slice(2);
if (!backend || !app) throw new Error('usage: node commands/reset-backend.mjs <backend> <app-dir>');

Promise.resolve().then(() => resetBackend({ backend, app })).then(result => {
  console.log(result);
}).catch(error => {
  if (error instanceof GeneratedAppLayoutError || error?.code === 'generated_app_layout') {
    console.error(`GENERATED_APP_LAYOUT: ${error.message}`);
    process.exitCode = GENERATED_APP_LAYOUT_EXIT_CODE;
    return;
  }
  console.error(error.stack ?? error.message);
  process.exitCode = 1;
});
