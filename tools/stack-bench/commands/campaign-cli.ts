#!/usr/bin/env node

import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { CAMPAIGN_MODE_REGISTRY } from '../src/campaigns/campaign-mode.js';
import { executeCampaign, inspectCampaign, reconcileCampaign }
  from '../src/campaigns/campaign-runner.js';
import { inspectCampaignSummary } from '../src/campaigns/campaign-inspection.js';
import { exportCampaignReport, generateCampaignReport } from '../src/campaigns/campaign-report.js';
import { grantCampaignDependencyRepairs }
  from '../src/campaigns/campaign-progression-grant.js';
import { requestCampaignTimeGrant } from '../src/campaigns/campaign-time-grant.js';
import { readCampaignProviderContinuationStatus, requestCampaignProviderContinuation }
  from '../src/campaigns/campaign-provider-continuation.js';
import { auditProgressionReferenceCampaign, formatProgressionReferenceCampaignAudit }
  from '../src/campaigns/progression-reference-campaign-audit.js';
import type { ReferenceCampaignAudit }
  from '../src/campaigns/progression-reference-campaign-audit.js';
import { prepareCampaignExtension } from '../src/campaigns/campaign-extension.js';
import { statusWord } from '../src/evidence/status-words.js';
import { readCampaignLock, requestCampaignCancellation } from '../src/campaigns/campaign-lock.js';

interface CampaignSummaryPlan {
  id: string;
  version: string;
  contentSha256: string;
}

interface CampaignSummaryState {
  status: string;
  summary: unknown;
  attempts: Array<{
    plan: { id: string };
    status: string;
    executions: Array<{
      id: string;
      outcome: unknown;
      reason: string | null;
    }>;
  }>;
}

interface ReferenceCampaignPlan {
  attempts: Array<{
    mode?: { id?: string };
    agentAdapter?: string;
  }>;
}

interface ReferenceCampaignState {
  status: string;
}

interface ResumeCampaign {
  plan: {
    contentSha256: string;
    definition: { mode?: { id?: string } };
  };
  state: {
    status: string;
    attempts: Array<{ executions: readonly unknown[] }>;
  };
}

type ReferenceCampaignAuditFunction = (directory: string) => ReferenceCampaignAudit | null;

export type CampaignArgs =
  | { command: 'continue-provider'; directory: string; attemptId: string; requestId: string }
  | { command: 'continuation-status'; directory: string; attemptId: string; json: boolean }
  | { command: 'grant-time'; directory: string; attemptId: string; grantId: string; minutes: number }
  | { command: 'modes' }
  | { command: 'validate'; path: string }
  | { command: 'show'; path: string }
  | { command: 'status'; directory: string; full: boolean }
  | { command: 'inspect'; directory: string }
  | { command: 'report'; directory: string }
  | { command: 'export'; directory: string; output: string }
  | { command: 'stop'; directory: string }
  | { command: 'audit'; directory: string }
  | { command: 'grant-repairs'; directory: string; attemptId: string; grantId: string;
    level: number; nodeIds: string[]; repairs: number }
  | { command: 'extend'; path: string; parentDirectory: string; fromDepth: number;
    directory: string }
  | { command: 'trial'; path: string; directory: string }
  | { command: 'run'; path: string; directory: string }
  | { command: 'resume'; path: string; directory: string }
  | { command: 'reconcile'; path: string; directory: string };

function isOneOf<const T extends string>(value: string | undefined,
  values: readonly T[]): value is T {
  return value !== undefined && values.some(candidate => candidate === value);
}

export function campaignStateSummary(plan: CampaignSummaryPlan, state: CampaignSummaryState) {
  const failures = state.attempts.flatMap(attempt => {
    const execution = attempt.executions.at(-1);
    if (!execution || execution.outcome === null || execution.outcome === 'passed') return [];
    return [{
      attempt: attempt.plan.id,
      status: statusWord(attempt.status),
      execution: execution.id,
      outcome: statusWord(String(execution.outcome)),
      reason: execution.reason,
    }];
  });
  return {
    campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
    status: statusWord(state.status),
    summary: state.summary,
    failures,
  };
}

export function auditCompletedReferenceCampaign(directory: string, plan: ReferenceCampaignPlan,
  state: ReferenceCampaignState, {
  audit = auditProgressionReferenceCampaign,
}: { audit?: ReferenceCampaignAuditFunction } = {}): ReferenceCampaignAudit | null {
  const hasReferenceProgression = plan.attempts.some(attempt =>
    attempt.mode?.id === 'dependency' && attempt.agentAdapter === 'reference-fixture');
  return state.status === 'completed' && hasReferenceProgression ? audit(directory) : null;
}

export function validateResumeCampaignState<T extends ResumeCampaign>(
  requested: { contentSha256: string }, existing: T): T {
  if (requested.contentSha256 !== existing.plan.contentSha256) {
    throw new Error('resume requires the exact campaign plan already stored in the output directory');
  }
  if (existing.plan.definition.mode?.id !== 'dependency') {
    throw new Error('resume is available only for dependency campaigns');
  }
  const executions = existing.state.attempts.reduce((total, attempt) =>
    total + attempt.executions.length, 0);
  if (existing.state.status !== 'prepared' || executions < 1) {
    throw new Error('resume requires a dependency campaign with scheduled work');
  }
  return existing;
}

export function validateResumeCampaign(path: string, directory: string): ResumeCampaign {
  return validateResumeCampaignState(compileCampaignFile(path), inspectCampaign(directory));
}

export function parseCampaignArgs(argv: string[]): CampaignArgs {
  const [command, path, ...rest] = argv.slice(2);
  if ((command === 'continue-provider' || command === 'continuation-status') && path) {
    const options = new Map<string, string>();
    let json = false;
    for (let i = 0; i < rest.length; i++) {
      const flag = rest[i]!;
      if (flag === '--json' && command === 'continuation-status' && !json) { json = true; continue; }
      if (!['--attempt', ...(command === 'continue-provider' ? ['--request-id'] : [])].includes(flag)
        || options.has(flag) || !rest[i + 1] || rest[i + 1]!.startsWith('--')) {
        throw new Error('invalid provider continuation options');
      }
      options.set(flag, rest[++i]!);
    }
    const attemptId = options.get('--attempt');
    if (!attemptId) throw new Error('provider continuation requires --attempt');
    if (command === 'continuation-status') return { command, directory: resolve(path), attemptId, json };
    const requestId = options.get('--request-id');
    if (!requestId) throw new Error('continue-provider requires --request-id');
    return { command, directory: resolve(path), attemptId, requestId };
  }
  if (command === 'modes' && path === undefined) return { command };
  if (isOneOf(command, ['validate', 'show']) && path && rest.length === 0) {
    return { command, path: resolve(path) };
  }
  if (command === 'status' && path
    && (rest.length === 0 || (rest.length === 1 && rest[0] === '--full'))) {
    return { command, directory: resolve(path), full: rest.length === 1 };
  }
  if (isOneOf(command, ['inspect', 'report', 'audit', 'stop']) && path && rest.length === 0) {
    return { command, directory: resolve(path) };
  }
  if (command === 'export' && path && rest.length === 2 && rest[0] === '--out' && rest[1]) {
    return { command, directory: resolve(path), output: resolve(rest[1]) };
  }
  if (command === 'grant-time' && path) {
    const options = new Map<string, string>();
    for (let i = 0; i < rest.length; i += 2) {
      const flag = rest[i]; const value = rest[i + 1];
      if (!flag || !['--attempt', '--grant-id', '--minutes'].includes(flag)
        || !value || options.has(flag)) throw new Error('invalid grant-time options');
      options.set(flag, value);
    }
    const minutes = Number(options.get('--minutes'));
    if (!options.get('--attempt') || !options.get('--grant-id')
      || !Number.isSafeInteger(minutes * 60_000) || !Number.isInteger(minutes) || minutes <= 0) {
      throw new Error('grant-time requires --attempt, --grant-id, --minutes <positive integer>');
    }
    return { command, directory: resolve(path), attemptId: options.get('--attempt')!,
      grantId: options.get('--grant-id')!, minutes };
  }
  if (command === 'grant-repairs' && path) {
    const values: { attemptId?: string; grantId?: string; level?: number; repairs?: number;
      nodeIds: string[] } = { nodeIds: [] };
    const seen = new Set<string>();
    for (let index = 0; index < rest.length; index += 2) {
      const flag = rest[index];
      const value = rest[index + 1];
      if (flag === undefined || value === undefined
        || !['--attempt', '--grant-id', '--level', '--feature', '--repairs'].includes(flag)
        || (flag !== '--feature' && seen.has(flag))) {
        throw new Error(`invalid or duplicate grant-repairs option ${String(flag)}`);
      }
      seen.add(flag);
      if (flag === '--attempt') values.attemptId = value;
      else if (flag === '--grant-id') values.grantId = value;
      else if (flag === '--level') values.level = Number(value);
      else if (flag === '--repairs') values.repairs = Number(value);
      else values.nodeIds.push(value);
    }
    if (!values.attemptId || !values.grantId || typeof values.level !== 'number'
      || !Number.isSafeInteger(values.level) || typeof values.repairs !== 'number'
      || !Number.isSafeInteger(values.repairs) || values.nodeIds.length === 0) {
      throw new Error('grant-repairs requires --attempt, --grant-id, --level, '
        + 'one or more --feature values, and --repairs');
    }
    return { command, directory: resolve(path), attemptId: values.attemptId,
      grantId: values.grantId, level: values.level, nodeIds: values.nodeIds,
      repairs: values.repairs };
  }
  if (command === 'extend' && path && rest.length === 6
    && rest[0] === '--from' && rest[2] === '--depth' && rest[4] === '--out') {
    const fromDepth = Number(rest[3]);
    if (!Number.isSafeInteger(fromDepth) || fromDepth < 1) {
      throw new Error('extend --depth must be a positive integer');
    }
    return { command, path: resolve(path), parentDirectory: resolve(rest[1]!),
      fromDepth, directory: resolve(rest[5]!) };
  }
  if (isOneOf(command, ['trial', 'run', 'resume', 'reconcile'])
    && path && rest.length === 2 && rest[0] === '--out') {
    return { command, path: resolve(path), directory: resolve(rest[1]!) };
  }
  throw new Error('usage: campaign-cli.js modes | validate|show <campaign.json> '
    + '| trial|run|resume|reconcile <campaign.json> --out <directory> '
    + '| extend <campaign.json> --from <campaign-directory> --depth <N> --out <directory> '
    + '| status <directory> [--full] | inspect|report|audit|stop <directory> | export <directory> --out <new-directory> '
    + '| grant-repairs <directory> --attempt <id> --grant-id <id> --level <N> '
    + '--feature <id> [--feature <id> ...] --repairs <N> '
    + '| grant-time <directory> --attempt <id> --grant-id <id> --minutes <N> '
    + '| continue-provider <directory> --attempt <id> --request-id <id> '
    + '| continuation-status <directory> --attempt <id> [--json]');
}

async function main() {
  const args = parseCampaignArgs(process.argv);
  if (args.command === 'modes') {
    console.log(JSON.stringify(CAMPAIGN_MODE_REGISTRY.ids.map(value => {
      const [id, version] = value.split('@');
      return { id, version };
    }), null, 2));
    return;
  }
  if (args.command === 'status') {
    const campaign = inspectCampaign(args.directory, { requireCurrentInputs: false });
    console.log(JSON.stringify(args.full
      ? campaign.state
      : campaignStateSummary(campaign.plan, campaign.state), null, 2));
    return;
  }
  if (args.command === 'inspect') {
    console.log(JSON.stringify(inspectCampaignSummary(args.directory), null, 2));
    return;
  }
  if (args.command === 'report') {
    const generated = generateCampaignReport(args.directory);
    console.log(`${generated.reportPath}\n${generated.htmlPath}\n${generated.report.contentSha256}`);
    return;
  }
  if (args.command === 'export') {
    console.log(exportCampaignReport(args.directory, args.output));
    return;
  }
  if (args.command === 'stop') {
    const lock = readCampaignLock(args.directory);
    if (!lock || !requestCampaignCancellation(args.directory,
      { id: lock.campaignId, contentSha256: lock.campaignSha256 }, lock.ownershipMarkerSha256)) {
      throw new Error('campaign has no current controller to stop');
    }
    console.log('Stop requested. The controller will stop its children and release owned resources.');
    return;
  }
  if (args.command === 'audit') {
    const report = auditProgressionReferenceCampaign(args.directory);
    if (report === null) throw new Error('campaign has no dependency reference attempts to audit');
    console.log(formatProgressionReferenceCampaignAudit(report));
    if (!report.ok) process.exitCode = 1;
    return;
  }
  if (args.command === 'continuation-status') {
    const status = readCampaignProviderContinuationStatus(args.directory, args.attemptId);
    console.log(args.json ? JSON.stringify(status, null, 2)
      : status.eligible ? 'Waiting: eligible for provider continuation.' : `Ineligible: ${status.reason}`);
    return;
  }
  if (args.command === 'continue-provider') {
    console.log(JSON.stringify(requestCampaignProviderContinuation(args.directory, args), null, 2));
    return;
  }
  if (args.command === 'grant-time') {
    console.log(JSON.stringify(requestCampaignTimeGrant(args.directory, {
      attemptId: args.attemptId, grantId: args.grantId, minutes: args.minutes }), null, 2));
    return;
  }
  if (args.command === 'grant-repairs') {
    console.log(JSON.stringify(grantCampaignDependencyRepairs(args.directory, {
      attemptId: args.attemptId,
      grantId: args.grantId,
      level: args.level,
      nodeIds: args.nodeIds,
      repairs: args.repairs,
    }), null, 2));
    return;
  }
  if (args.command === 'extend') {
    prepareCampaignExtension(args.path, args.parentDirectory, args.directory, args.fromDepth);
    const plan = compileCampaignFile(args.path);
    const state = await executeCampaign(args.path, args.directory, { mode: 'frozen' });
    console.log(JSON.stringify(campaignStateSummary(plan, state), null, 2));
    if (state.status !== 'completed') process.exitCode = 1;
    return;
  }
  const plan = compileCampaignFile(args.path);
  if (args.command === 'reconcile') {
    const state = reconcileCampaign(args.path, args.directory);
    console.log(JSON.stringify(campaignStateSummary(plan, state), null, 2));
    return;
  }
  if (args.command === 'trial' || args.command === 'run' || args.command === 'resume') {
    if (args.command === 'resume') validateResumeCampaign(args.path, args.directory);
    const cancellation = new AbortController();
    const cancel = () => cancellation.abort();
    process.on('SIGINT', cancel);
    process.on('SIGTERM', cancel);
    let state;
    try {
      const executionMode = args.command === 'trial'
        || (args.command === 'resume' && plan.state === 'draft')
        ? 'model-free-trial' : 'frozen';
      state = await executeCampaign(args.path, args.directory, {
        mode: executionMode,
        signal: cancellation.signal,
      });
    } finally {
      process.off('SIGINT', cancel);
      process.off('SIGTERM', cancel);
    }
    console.log(JSON.stringify(campaignStateSummary(plan, state), null, 2));
    const audit = auditCompletedReferenceCampaign(args.directory, plan, state);
    if (audit !== null) console.log(formatProgressionReferenceCampaignAudit(audit));
    if (state.status !== 'completed' || audit?.ok === false) process.exitCode = 1;
    return;
  }
  if (args.command === 'show') console.log(JSON.stringify(plan, null, 2));
  else console.log(`${plan.id}@${plan.version} ${plan.state}: ${plan.summary.attempts} attempts, ${plan.contentSha256}`);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  main().catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 2;
  });
}
