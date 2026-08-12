#!/usr/bin/env node

import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { compileCampaignFile } from './campaign-compiler.mjs';

export function parseCampaignArgs(argv) {
  const [command, path, ...rest] = argv.slice(2);
  if (!['validate', 'show'].includes(command) || !path || rest.length) {
    throw new Error('usage: campaign-cli.mjs validate|show <campaign.json>');
  }
  return { command, path: resolve(path) };
}

function main() {
  const args = parseCampaignArgs(process.argv);
  const plan = compileCampaignFile(args.path);
  if (args.command === 'show') console.log(JSON.stringify(plan, null, 2));
  else console.log(`${plan.id}@${plan.version} ${plan.state}: ${plan.summary.attempts} attempts, ${plan.contentSha256}`);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try { main(); }
  catch (error) { console.error(error.message); process.exitCode = 2; }
}
