#!/usr/bin/env node

import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { compileCampaignFile } from '../src/campaigns/campaign-compiler.mjs';
import { CAMPAIGN_MODE_REGISTRY } from '../src/campaigns/campaign-mode.mjs';
import { executeCampaign, inspectCampaign, prepareCampaign, reconcileCampaign }
  from '../src/campaigns/campaign-runner.mjs';
import { inspectCampaignSummary } from '../src/campaigns/campaign-inspection.mjs';
import { generateCampaignReport } from '../src/campaigns/campaign-report.mjs';
import { auditProgressionReferenceCampaign, formatProgressionReferenceCampaignAudit }
  from '../src/campaigns/progression-reference-campaign-audit.mjs';

export function campaignStateSummary(plan, state) {
  const failures = state.attempts.flatMap(attempt => {
    const execution = attempt.executions.at(-1);
    if (!execution || execution.outcome === null || execution.outcome === 'passed') return [];
    return [{
      attempt: attempt.plan.id,
      status: attempt.status,
      execution: execution.id,
      outcome: execution.outcome,
      reason: execution.reason,
    }];
  });
  return {
    campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
    status: state.status,
    summary: state.summary,
    failures,
  };
}

export function auditCompletedReferenceCampaign(directory, plan, state, {
  audit = auditProgressionReferenceCampaign,
} = {}) {
  const hasReferenceProgression = plan.attempts.some(attempt =>
    attempt.mode?.id === 'dependency' && attempt.agentAdapter === 'reference-fixture');
  return state.status === 'completed' && hasReferenceProgression ? audit(directory) : null;
}

export function validateResumeCampaignState(requested, existing) {
  if (requested.contentSha256 !== existing.plan.contentSha256) {
    throw new Error('resume requires the exact campaign plan already stored in the output directory');
  }
  if (existing.plan.definition.mode?.id !== 'dependency') {
    throw new Error('resume is available only for dependency campaigns');
  }
  const executions = existing.state.attempts.reduce((total, attempt) =>
    total + attempt.executions.length, 0);
  if (existing.state.status !== 'prepared' || executions < 1) {
    throw new Error('resume requires an interrupted dependency campaign that is ready');
  }
  return existing;
}

export function validateResumeCampaign(path, directory) {
  return validateResumeCampaignState(compileCampaignFile(path), inspectCampaign(directory));
}

export function parseCampaignArgs(argv) {
  const [command, path, ...rest] = argv.slice(2);
  if (command === 'modes' && path === undefined) return { command };
  if (['validate', 'show'].includes(command) && path && rest.length === 0) {
    return { command, path: resolve(path) };
  }
  if (command === 'status' && path
    && (rest.length === 0 || (rest.length === 1 && rest[0] === '--full'))) {
    return { command, directory: resolve(path), full: rest.length === 1 };
  }
  if (['inspect', 'report', 'audit'].includes(command) && path && rest.length === 0) {
    return { command, directory: resolve(path) };
  }
  if (['prepare', 'trial', 'run', 'resume', 'reconcile'].includes(command)
    && path && rest.length === 2 && rest[0] === '--out') {
    return { command, path: resolve(path), directory: resolve(rest[1]) };
  }
  throw new Error('usage: campaign-cli.mjs modes | validate|show <campaign.json> '
    + '| prepare|trial|run|resume|reconcile <campaign.json> --out <directory> '
    + '| status <directory> [--full] | inspect|report|audit <directory>');
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
  if (args.command === 'audit') {
    const report = auditProgressionReferenceCampaign(args.directory);
    if (report === null) throw new Error('campaign has no dependency reference attempts to audit');
    console.log(formatProgressionReferenceCampaignAudit(report));
    if (!report.ok) process.exitCode = 1;
    return;
  }
  if (args.command === 'prepare') {
    const prepared = prepareCampaign(args.path, args.directory);
    console.log(JSON.stringify(campaignStateSummary(prepared.plan, prepared.state), null, 2));
    return;
  }
  const plan = compileCampaignFile(args.path);
  if (args.command === 'reconcile') {
    const state = reconcileCampaign(args.path, args.directory);
    console.log(JSON.stringify(campaignStateSummary(plan, state), null, 2));
    return;
  }
  if (['trial', 'run', 'resume'].includes(args.command)) {
    if (args.command === 'resume') validateResumeCampaign(args.path, args.directory);
    const state = await executeCampaign(args.path, args.directory,
      { mode: args.command === 'trial' ? 'model-free-trial' : 'frozen' });
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
  main().catch(error => { console.error(error.message); process.exitCode = 2; });
}
