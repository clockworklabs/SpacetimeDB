#!/usr/bin/env node

import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { compileCampaignFile } from './campaign-compiler.mjs';
import { executeCampaign, inspectCampaign, prepareCampaign, reconcileCampaign } from './campaign-runner.mjs';

export function parseCampaignArgs(argv) {
  const [command, path, ...rest] = argv.slice(2);
  if (['validate', 'show'].includes(command) && path && rest.length === 0) {
    return { command, path: resolve(path) };
  }
  if (command === 'status' && path && rest.length === 0) {
    return { command, directory: resolve(path) };
  }
  if (['prepare', 'run', 'reconcile'].includes(command) && path && rest.length === 2 && rest[0] === '--out') {
    return { command, path: resolve(path), directory: resolve(rest[1]) };
  }
  throw new Error('usage: campaign-cli.mjs validate|show <campaign.json> | prepare|run|reconcile <campaign.json> --out <directory> | status <directory>');
}

async function main() {
  const args = parseCampaignArgs(process.argv);
  if (args.command === 'status') {
    console.log(JSON.stringify(inspectCampaign(args.directory).state, null, 2));
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
  if (args.command === 'run') {
    const state = await executeCampaign(args.path, args.directory);
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
