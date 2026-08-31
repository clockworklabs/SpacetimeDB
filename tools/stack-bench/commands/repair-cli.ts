#!/usr/bin/env node

import { randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { acquireCampaignLock, releaseCampaignLock } from '../src/campaigns/campaign-lock.js';
import { emptyArtifactIdentities, readArtifact, writeArtifact } from '../src/evidence/artifacts.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { rescueSupervisedLease } from '../src/runtime/recovery.js';
import { runBounded } from '../src/runtime/bounded-process.js';
import type { BoundedProcessResult, RunBoundedOptions }
  from '../src/runtime/bounded-process.js';
import { createRepairGrant, inspectRepairParent } from '../src/runtime/repair-grant.js';

const BENCH = join(STACK_BENCH_ROOT, 'dist', 'commands', 'bench.js');

export interface RepairStatusArgs {
  command: 'status';
  parent: string;
  level: number;
}

export interface RepairGrantArgs {
  command: 'grant';
  parent: string;
  level: number;
  rounds: number;
  maxBudgetUsd?: number;
  timeoutMinutes: number;
}

export type RepairArgs = RepairStatusArgs | RepairGrantArgs;

export function parseRepairArgs(argv: string[]): RepairArgs {
  const [command, parent, ...rest] = argv.slice(2);
  if (command === 'status' && parent && rest.length === 2 && rest[0] === '--level') {
    const level = Number(rest[1]);
    if (!Number.isSafeInteger(level) || level < 1) throw new Error('--level must be a positive integer');
    return { command, parent: resolve(parent), level };
  }
  if (command !== 'grant' || !parent) {
    throw new Error('usage: repair status <run-dir> --level <N> | repair grant <run-dir> --level <N> --rounds <N> [--max-budget-usd <N>] [--timeout-minutes <N>]');
  }
  const values: { level?: number; rounds?: number; maxBudgetUsd?: number;
    timeoutMinutes: number } = { timeoutMinutes: 120 };
  const seen = new Set<string>();
  for (let index = 0; index < rest.length; index += 2) {
    const flag = rest[index];
    if (!flag || !['--level', '--rounds', '--max-budget-usd', '--timeout-minutes'].includes(flag)
      || index + 1 >= rest.length || seen.has(flag)) {
      throw new Error(`invalid or duplicate repair option ${String(flag)}`);
    }
    seen.add(flag);
    const value = Number(rest[index + 1]);
    if (flag === '--level') values.level = value;
    else if (flag === '--rounds') values.rounds = value;
    else if (flag === '--max-budget-usd') values.maxBudgetUsd = value;
    else values.timeoutMinutes = value;
  }
  const level = values.level;
  if (level === undefined || !Number.isSafeInteger(level) || level < 1) {
    throw new Error('--level must be a positive integer');
  }
  const rounds = values.rounds;
  if (rounds === undefined || !Number.isSafeInteger(rounds) || rounds < 1 || rounds > 20) {
    throw new Error('--rounds must be an integer from 1 through 20');
  }
  if (values.maxBudgetUsd !== undefined
    && (!Number.isFinite(values.maxBudgetUsd) || values.maxBudgetUsd <= 0)) {
    throw new Error('--max-budget-usd must be a positive number');
  }
  if (!Number.isFinite(values.timeoutMinutes) || values.timeoutMinutes < 10
    || values.timeoutMinutes > 480) {
    throw new Error('--timeout-minutes must be from 10 through 480');
  }
  return { command, parent: resolve(parent), level,
    rounds, timeoutMinutes: values.timeoutMinutes,
    ...(values.maxBudgetUsd === undefined ? {} : { maxBudgetUsd: values.maxBudgetUsd }) };
}

export function repairStatus(parent: string, level: number): Record<string, unknown> {
  try {
    const inspected = inspectRepairParent(parent, level);
    return { eligible: true, parentRunId: inspected.parent.id, level,
      score: inspected.level.score, max: inspected.level.max,
      roundsUsed: inspected.cumulativeRoundsBefore,
      checkpointSha256: inspected.checkpoint.payload.source.sha256 };
  } catch (error) {
    return { eligible: false, level,
      reason: error instanceof Error ? error.message : String(error) };
  }
}

interface RepairExecutionDependencies {
  execute?: (command: string, argv: string[],
    options: RunBoundedOptions) => Promise<BoundedProcessResult>;
  rescue?: (path: string, output: string) => void;
  uuid?: () => string;
  env?: NodeJS.ProcessEnv;
}

interface RepairContinuationPayload {
  outcome?: unknown;
  continuation?: {
    parentRunId?: string;
    roundsGranted?: number;
    level?: number;
    [key: string]: unknown;
  };
  [key: string]: unknown;
}

export async function executeRepairGrant(args: RepairGrantArgs,
  { execute = runBounded, rescue = rescueSupervisedLease, uuid = randomUUID,
    env = process.env }: RepairExecutionDependencies = {}) {
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
    const childEnv: NodeJS.ProcessEnv = {
      ...env,
      STACK_BENCH_SUPERVISOR_STATE: supervisorState,
    };
    if (resolved.configuration.buildImage) {
      childEnv.STACK_BENCH_IMAGE = resolved.configuration.buildImage;
    }
    const processResult = await execute(process.execPath, argv, {
      cwd: STACK_BENCH_ROOT,
      env: childEnv,
      stdio: 'inherit',
      timeoutMs: args.timeoutMinutes * 60_000,
      logs: { stdout: join(output, 'process.stdout.log'),
        stderr: join(output, 'process.stderr.log') },
    });
    let cleanupError: unknown = null;
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
        timedOut: processResult.timedOut, streams },
    });
    if (cleanupError) {
      const detail = cleanupError instanceof Error ? cleanupError.message : String(cleanupError);
      throw new Error(`repair continuation cleanup failed: ${detail}`);
    }
    const runPath = join(output, 'run.json');
    if (!existsSync(runPath)) {
      throw new Error(`repair continuation produced no run artifact${processResult.timedOut ? ' before its timeout' : ''}`);
    }
    const run = readArtifact<RepairContinuationPayload>(runPath,
      { expectedKind: 'repair_continuation' });
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

async function main(): Promise<void> {
  const args = parseRepairArgs(process.argv);
  if (args.command === 'status') {
    const status = repairStatus(args.parent, args.level);
    console.log(JSON.stringify(status, null, 2));
    if (status.eligible !== true) process.exitCode = 1;
    return;
  }
  const result = await executeRepairGrant(args);
  console.log(JSON.stringify({ output: result.output, id: result.run.id,
    outcome: result.run.payload.outcome,
    continuation: result.run.payload.continuation }, null, 2));
  if (!result.process.ok) process.exitCode = 1;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  main().catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 2;
  });
}
