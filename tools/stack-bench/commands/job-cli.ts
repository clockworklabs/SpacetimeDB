#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { cancelExecutionJob, listExecutionJobs, readExecutionJob,
  submitExecutionJob, workExecutionJob } from '../src/campaigns/execution-jobs.js';
import { runExecutionWorker } from '../src/campaigns/execution-worker.js';
import { prepareRun, runSetupCatalog, submitPreparedRun } from '../src/campaigns/run-setup.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { executeCampaign, inspectCampaign } from '../src/campaigns/campaign-runner.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { stackBenchResultsRoot } from '../src/runtime/operational-paths.js';
import { validateResumeCampaignState } from './campaign-cli.js';

// Every command also takes --results.
const COMMAND_OPTIONS: Record<string, readonly string[]> = {
  options: [], prepare: [], start: ['host'], submit: [], list: ['after', 'limit'],
  status: [], cancel: [], resume: [], work: ['host'], worker: ['host', 'concurrency'],
};

/** Continue a job's interrupted campaign with the accounts and capacity policy the job saved. */
export async function resumeExecutionJob(results: string, id: string,
  { env = process.env, signal, execute = executeCampaign, inspect = inspectCampaign }:
  { env?: NodeJS.ProcessEnv; signal?: AbortSignal; execute?: typeof executeCampaign;
    inspect?: typeof inspectCampaign } = {}) {
  const { job, campaignDirectory } = readExecutionJob(results, id);
  const planFile = join(resolve(results), 'jobs', job.id, 'plan.json');
  const plan = compileCampaignFile(planFile);
  validateResumeCampaignState(plan, inspect(campaignDirectory));
  const state = await execute(planFile, campaignDirectory, {
    mode: plan.state === 'draft' ? 'model-free-trial' : 'frozen',
    env, signal, executionCredentials: job.credentials, capacityPolicy: job.capacityPolicy,
  });
  return { status: state.status, campaignDirectory, campaign: state.summary };
}

export async function jobCommand(argv: string[], env: NodeJS.ProcessEnv = process.env) {
  const { values, positionals } = parseArgs({ args: argv, allowPositionals: true, options: {
    results: { type: 'string' }, host: { type: 'string' }, after: { type: 'string' },
    limit: { type: 'string' }, concurrency: { type: 'string' },
  } });
  const [command, argument] = positionals;
  if (argv[0] !== command) throw new Error('put the job command before its options');
  if (positionals.length > 2) throw new Error('unexpected job arguments');
  if (command && Object.hasOwn(COMMAND_OPTIONS, command)) {
    const extra = Object.keys(values).find(option => option !== 'results'
      && !COMMAND_OPTIONS[command]!.includes(option));
    if (extra) throw new Error(`job ${command} does not take --${extra}`);
  }
  const results = values.results ? resolve(values.results) : stackBenchResultsRoot(STACK_BENCH_ROOT, env);
  if (command === 'options' && !argument) return runSetupCatalog(results, env);
  if (command === 'prepare' && argument) return prepareRun(results,
    JSON.parse(readFileSync(argument === '-' ? 0 : argument, 'utf8')), env);
  if (command === 'start' && argument) {
    const host = values.host ?? env.STACK_BENCH_HOST_ID;
    if (!host) throw new Error('job start requires --host or STACK_BENCH_HOST_ID');
    const job = submitPreparedRun(results, JSON.parse(readFileSync(argument === '-' ? 0 : argument, 'utf8')), env);
    console.log(JSON.stringify({ jobId: job.id, campaignKey: `job-${job.id}` }));
    return jobCommand(['work', job.id, '--results', results, '--host', host], env);
  }
  if (command === 'submit' && argument) return submitExecutionJob(results,
    JSON.parse(readFileSync(argument === '-' ? 0 : argument, 'utf8')));
  if (command === 'status' && argument) return readExecutionJob(results, argument);
  if (command === 'cancel' && argument) {
    cancelExecutionJob(results, argument); return readExecutionJob(results, argument);
  }
  if (command === 'list' && !argument) return listExecutionJobs(results,
    { after: values.after, limit: values.limit === undefined ? undefined : Number(values.limit) });
  if ((command === 'work' && argument) || (command === 'worker' && !argument)
    || (command === 'resume' && argument)) {
    const host = values.host ?? env.STACK_BENCH_HOST_ID;
    if (!host && command !== 'resume') throw new Error('job work/worker requires --host or STACK_BENCH_HOST_ID');
    const controller = new AbortController();
    const stop = () => controller.abort();
    process.on('SIGTERM', stop); process.on('SIGINT', stop);
    try {
      if (command === 'resume') return await resumeExecutionJob(results, argument!, { env, signal: controller.signal });
      if (command === 'worker') {
        await runExecutionWorker(results, host!, { env, signal: controller.signal,
          concurrency: Number(values.concurrency) });
        return { status: 'stopped' as const };
      }
      return await workExecutionJob(results, argument!, host!, { env, signal: controller.signal });
    }
    finally { process.off('SIGTERM', stop); process.off('SIGINT', stop); }
  }
  throw new Error('use job options, prepare <json|->, start <review-json|-> --host <host>, submit <json|->, list, status <id>, cancel <id>, resume <id>, work <id> --host <host>, or worker --host <host> --concurrency <jobs>');
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  jobCommand(process.argv.slice(2)).then(result => {
    console.log(JSON.stringify(result, null, 2));
    if ('status' in result && result.status === 'failed') process.exitCode = 1;
  }).catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : String(error)); process.exitCode = 1;
  });
}
