#!/usr/bin/env node

import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { compileCampaignFile } from '../src/campaigns/campaign-compiler.mjs';
import { CAMPAIGN_MODE_REGISTRY } from '../src/campaigns/campaign-mode.mjs';
import { executeCampaign, inspectCampaign, prepareCampaign, reconcileCampaign } from '../src/campaigns/campaign-runner.mjs';
import { inspectCampaignSummary } from '../src/campaigns/campaign-inspection.mjs';
import { generateCampaignReport } from '../src/campaigns/campaign-report.mjs';

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
  if (['status', 'inspect', 'report'].includes(command) && path && rest.length === 0) {
    return { command, directory: resolve(path) };
  }
  if (['prepare', 'trial', 'run', 'resume', 'reconcile'].includes(command)
    && path && rest.length === 2 && rest[0] === '--out') {
    return { command, path: resolve(path), directory: resolve(rest[1]) };
  }
  throw new Error('usage: campaign-cli.mjs modes | validate|show <campaign.json> | prepare|trial|run|resume|reconcile <campaign.json> --out <directory> | status|inspect|report <directory>');
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
    console.log(JSON.stringify(inspectCampaign(args.directory).state, null, 2));
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
  if (args.command === 'prepare') {
    const prepared = prepareCampaign(args.path, args.directory);
    console.log(JSON.stringify(prepared.state, null, 2));
    return;
  }
  const plan = compileCampaignFile(args.path);
  if (args.command === 'reconcile') {
    console.log(JSON.stringify(reconcileCampaign(args.path, args.directory), null, 2));
    return;
  }
  if (['trial', 'run', 'resume'].includes(args.command)) {
    if (args.command === 'resume') validateResumeCampaign(args.path, args.directory);
    const state = await executeCampaign(args.path, args.directory,
      { mode: args.command === 'trial' ? 'model-free-trial' : 'frozen' });
    console.log(JSON.stringify(state, null, 2));
    if (state.status !== 'completed') process.exitCode = 1;
    return;
  }
  if (args.command === 'show') console.log(JSON.stringify(plan, null, 2));
  else console.log(`${plan.id}@${plan.version} ${plan.state}: ${plan.summary.attempts} attempts, ${plan.contentSha256}`);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  main().catch(error => { console.error(error.message); process.exitCode = 2; });
}
