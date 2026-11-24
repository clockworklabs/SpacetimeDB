/**
 * Location of the local checkout of clockworklabs/spacetimedb-llm-oneshot-results,
 * where graded benchmark apps are published. Generated apps are never committed
 * to the SpacetimeDB repo.
 *
 * Resolution order:
 *   1. --results-repo <path>
 *   2. LLM_ONESHOT_RESULTS_REPO environment variable
 *   3. A sibling of the SpacetimeDB checkout: ../spacetimedb-llm-oneshot-results
 */

import { execFileSync } from 'node:child_process';
import * as fs from 'node:fs';
import * as path from 'node:path';

export const RESULTS_REPO_URL =
  'git@github.com:clockworklabs/spacetimedb-llm-oneshot-results.git';

/** tools/llm-oneshot */
export function toolDir(): string {
  const scriptDir = path.dirname(new URL(import.meta.url).pathname);
  // Handle Windows paths (remove leading slash if present)
  return path.resolve(scriptDir.replace(/^\/([A-Za-z]:)/, '$1'), '..');
}

export function resultsRepoPath(explicit?: string): string {
  if (explicit) return path.resolve(explicit);
  const fromEnv = process.env.LLM_ONESHOT_RESULTS_REPO;
  if (fromEnv) return path.resolve(fromEnv);
  const spacetimeRoot = path.resolve(toolDir(), '../..');
  return path.resolve(spacetimeRoot, '..', 'spacetimedb-llm-oneshot-results');
}

export function git(cwd: string, ...args: string[]): string {
  return execFileSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] }).trim();
}

/** Returns the checkout path, cloning it first if it does not exist. */
export function ensureResultsRepo(explicit?: string): string {
  const repo = resultsRepoPath(explicit);
  if (!fs.existsSync(repo)) {
    console.log(`Cloning ${RESULTS_REPO_URL} into ${repo}`);
    execFileSync('git', ['clone', RESULTS_REPO_URL, repo], { stdio: 'inherit' });
  }
  if (!fs.existsSync(path.join(repo, '.git'))) {
    throw new Error(`${repo} exists but is not a git checkout`);
  }
  return repo;
}
