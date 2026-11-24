#!/usr/bin/env tsx
/**
 * Publishes a graded benchmark app from local staging to
 * clockworklabs/spacetimedb-llm-oneshot-results.
 *
 *   apps/<app>/staging/<lang>/<llm>/<backend>/<run>/   (local, gitignored)
 *     -> <results repo>/apps/<app>/<lang>/<llm>/<backend>/<run>/
 *
 * The app must contain GRADING_RESULTS.md. The script pulls the results repo,
 * copies the app (skipping build output and dependencies), commits, and pushes.
 *
 * Usage:
 *   pnpm run publish-result -- apps/chat-app/staging/typescript/opus-4-5/spacetime/chat-app-20260108-120000
 *   pnpm run publish-result -- <path> --results-repo ../../../spacetimedb-llm-oneshot-results
 *   pnpm run publish-result -- <path> --no-push
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import { ensureResultsRepo, git } from './results-repo.ts';

// Never published: dependencies, build output, generated bindings, secrets.
const SKIP = new Set([
  'node_modules',
  'dist',
  'build',
  'target',
  'bin',
  'obj',
  'module_bindings',
  'playwright-report',
  'test-results',
  '.env',
  '.env.local',
  '.spacetime_token',
  '.DS_Store',
]);

function parseArgs() {
  const args = process.argv.slice(2);
  let appPath: string | undefined;
  let resultsRepo: string | undefined;
  let push = true;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--results-repo' && args[i + 1]) resultsRepo = args[++i];
    else if (args[i] === '--no-push') push = false;
    else if (!args[i].startsWith('--')) appPath = args[i];
  }
  if (!appPath) {
    console.error('Usage: publish-result <apps/<app>/staging/...> [--results-repo <path>] [--no-push]');
    process.exit(1);
  }
  return { appPath: path.resolve(appPath), resultsRepo, push };
}

function main() {
  const { appPath, resultsRepo, push } = parseArgs();

  // .../apps/<app>/staging/<lang>/<llm>/<backend>/<run>
  const parts = appPath.split(path.sep);
  const staging = parts.lastIndexOf('staging');
  if (staging < 2 || parts[staging - 2] !== 'apps' || parts.length - staging !== 5) {
    throw new Error(`Expected apps/<app>/staging/<lang>/<llm>/<backend>/<run>, got ${appPath}`);
  }
  if (!fs.existsSync(path.join(appPath, 'GRADING_RESULTS.md'))) {
    throw new Error(`${appPath} has no GRADING_RESULTS.md; grade it before publishing`);
  }
  const relative = path.join('apps', parts[staging - 1], ...parts.slice(staging + 1));

  const repo = ensureResultsRepo(resultsRepo);
  if (git(repo, 'status', '--porcelain')) {
    throw new Error(`${repo} has uncommitted changes; commit or discard them first`);
  }
  git(repo, 'pull', '--ff-only');

  const dest = path.join(repo, relative);
  if (fs.existsSync(dest)) throw new Error(`${dest} already exists`);
  fs.cpSync(appPath, dest, {
    recursive: true,
    filter: src => !SKIP.has(path.basename(src)),
  });

  git(repo, 'add', '--', relative);
  git(repo, 'commit', '-m', `Add ${relative}`);
  console.log(`Committed ${relative} in ${repo}`);
  if (push) {
    git(repo, 'push');
    console.log('Pushed.');
  } else {
    console.log('Not pushed (--no-push).');
  }
}

main();
