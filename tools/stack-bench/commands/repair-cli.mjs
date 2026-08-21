#!/usr/bin/env node

import { randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { pathToFileURL } from 'node:url';

import { emptyArtifactIdentities, readArtifact, writeArtifact } from '../src/evidence/artifacts.mjs';
import { acquireCampaignLock, releaseCampaignLock } from '../src/campaigns/campaign-lock.mjs';
import { createRepairGrant, inspectRepairParent } from '../src/runtime/repair-grant.mjs';
import { rescueSupervisedLease, runBounded } from '../src/references/reference-live.mjs';

const ROOT = import.meta.dirname;
const BENCH = join(ROOT, 'commands', 'bench.mjs');

export function parseRepairArgs(argv) {
  const [command, parent, ...rest] = argv.slice(2);
  if (command === 'status' && parent && rest.length === 2 && rest[0] === '--level') {
    const level = Number(rest[1]);
    if (!Number.isSafeInteger(level) || level < 1) throw new Error('--level must be a positive integer');
    return { command, parent: resolve(parent), level };
  }
  if (command !== 'grant' || !parent) {
    throw new Error('usage: repair status <run-dir> --level <N> | repair grant <run-dir> --level <N> --rounds <N> [--max-budget-usd <N>] [--timeout-minutes <N>]');
  }
  const args = { command, parent: resolve(parent), timeoutMinutes: 120 };
  const seen = new Set();
  for (let i = 0; i < rest.length; i += 2) {
    const flag = rest[i];
    if (!['--level', '--rounds', '--max-budget-usd', '--timeout-minutes'].includes(flag)
      || i + 1 >= rest.length || seen.has(flag)) throw new Error(`invalid or duplicate repair option ${flag}`);
    seen.add(flag);
    const value = Number(rest[i + 1]);
    if (flag === '--level') args.level = value;
    else if (flag === '--rounds') args.rounds = value;
    else if (flag === '--max-budget-usd') args.maxBudgetUsd = value;
    else args.timeoutMinutes = value;
  }
  if (!Number.isSafeInteger(args.level) || args.level < 1) throw new Error('--level must be a positive integer');
  if (!Number.isSafeInteger(args.rounds) || args.rounds < 1 || args.rounds > 20) {
    throw new Error('--rounds must be an integer from 1 through 20');
  }
  if (args.maxBudgetUsd !== undefined
    && (!Number.isFinite(args.maxBudgetUsd) || args.maxBudgetUsd <= 0)) {
    throw new Error('--max-budget-usd must be a positive number');
  }
  if (!Number.isFinite(args.timeoutMinutes) || args.timeoutMinutes < 10 || args.timeoutMinutes > 480) {
    throw new Error('--timeout-minutes must be from 10 through 480');
  }
  return args;
}

export function repairStatus(parent, level) {
  try {
    const inspected = inspectRepairParent(parent, level);
    return { eligible: true, parentRunId: inspected.parent.id, level,
      score: inspected.level.score, max: inspected.level.max,
      roundsUsed: inspected.cumulativeRoundsBefore,
      checkpointSha256: inspected.checkpoint.payload.source.sha256 };
  } catch (error) {
    return { eligible: false, level, reason: error.message };
  }
}

export async function executeRepairGrant(args, { execute = runBounded,
  rescue = rescueSupervisedLease, uuid = randomUUID, env = process.env } = {}) {
  const resolved = createRepairGrant(args.parent, { level: args.level, rounds: args.rounds });
  const lock = acquireCampaignLock(join(resolved.root, '.repair-control'), {
    id: `repair-l${args.level}`,
    contentSha256: resolved.checkpoint.payload.source.sha256,
  });
  const stamp = new Date().toISOString().replace(/[-:.TZ]/g, '').slice(0, 14);
  const executionId = `grant-${stamp}-${uuid().replaceAll('-', '').slice(0, 12)}`;
  const output = join(resolved.root, 'continuations', executionId);
  const privateRoot = join(tmpdir(), 'stack-bench-repair-supervisors');
  const supervisorState = join(privateRoot, `${executionId}.json`);
  try {
    mkdirSync(output, { recursive: true });
    mkdirSync(privateRoot, { recursive: true, mode: 0o700 });
    const argv = [BENCH,
      '--repair-from', resolved.root,
      '--repair-level', String(args.level),
      '--fix-rounds', String(args.rounds),
      '--out', output,
      '--no-media'];
    if (args.maxBudgetUsd !== undefined) {
      argv.push('--max-budget-usd', String(args.maxBudgetUsd));
    }
    const childEnv = { ...env, STACK_BENCH_SUPERVISOR_STATE: supervisorState };
    if (resolved.configuration.buildImage) childEnv.STACK_BENCH_IMAGE = resolved.configuration.buildImage;
    const processResult = await execute(process.execPath, argv, {
      cwd: ROOT,
      env: childEnv,
      stdio: 'inherit',
      timeoutMs: args.timeoutMinutes * 60_000,
      logs: { stdout: join(output, 'process.stdout.log'), stderr: join(output, 'process.stderr.log') },
    });
    let cleanupError = null;
    if (!processResult.ok && existsSync(supervisorState)) {
      try { rescue(supervisorState, output); }
      catch (error) { cleanupError = error; }
    }
    const streams = processResult.logs ? Object.fromEntries(Object.entries(processResult.logs)
      .map(([name, value]) => [name, { ...value, path: `process.${name}.log` }])) : null;
    writeArtifact(join(output, 'process.json'), {
      kind: 'repair_process',
      id: `${executionId}-process`,
      attempt: { id: `${executionId}-process`, parentId: resolved.parent.id },
      identities: emptyArtifactIdentities({
        agentAdapter: resolved.parentArtifact.identities.agentAdapter,
        stackAdapter: resolved.parentArtifact.identities.stackAdapter,
      }),
      payload: { schemaVersion: 1, parentRunId: resolved.parent.id,
        level: args.level, roundsGranted: args.rounds,
        exitCode: processResult.code ?? null, signal: processResult.signal ?? null,
        timedOut: processResult.timedOut === true, streams },
    });
    if (cleanupError) throw new Error(`repair continuation cleanup failed: ${cleanupError.message}`);
    const runPath = join(output, 'run.json');
    if (!existsSync(runPath)) {
      throw new Error(`repair continuation produced no run artifact${processResult.timedOut ? ' before its timeout' : ''}`);
    }
    const run = readArtifact(runPath, { expectedKind: 'repair_continuation' });
    if (run.attempt.parentId !== resolved.parent.id
      || run.payload.continuation?.parentRunId !== resolved.parent.id
      || run.payload.continuation?.roundsGranted !== args.rounds
      || run.payload.continuation?.level !== args.level) {
      throw new Error('repair continuation result does not match its grant');
    }
    return { output, process: processResult, run };
  } finally {
    rmSync(supervisorState, { force: true });
    releaseCampaignLock(lock);
  }
}

async function main() {
  const args = parseRepairArgs(process.argv);
  if (args.command === 'status') {
    const status = repairStatus(args.parent, args.level);
    console.log(JSON.stringify(status, null, 2));
    if (!status.eligible) process.exitCode = 1;
    return;
  }
  const result = await executeRepairGrant(args);
  console.log(JSON.stringify({ output: result.output, id: result.run.id,
    outcome: result.run.payload.outcome,
    continuation: result.run.payload.continuation }, null, 2));
  if (!result.process.ok) process.exitCode = 1;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  main().catch(error => { console.error(error.message); process.exitCode = 2; });
}
