#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { cancelExecutionJob, listExecutionJobs, readExecutionJob,
  submitExecutionJob, workExecutionJob } from '../src/campaigns/execution-jobs.js';

export async function jobCommand(argv: string[], env: NodeJS.ProcessEnv = process.env) {
  const { values, positionals } = parseArgs({ args: argv, allowPositionals: true, options: {
    results: { type: 'string' }, host: { type: 'string' }, after: { type: 'string' },
    limit: { type: 'string' },
  } });
  const [command, argument] = positionals;
  if (positionals.length > 2) throw new Error('unexpected job arguments');
  const results = resolve(values.results ?? env.STACK_BENCH_RESULTS_DIR ?? 'results');
  if (command === 'submit' && argument) return submitExecutionJob(results,
    JSON.parse(readFileSync(argument === '-' ? 0 : argument, 'utf8')));
  if (command === 'status' && argument) return readExecutionJob(results, argument);
  if (command === 'cancel' && argument) {
    cancelExecutionJob(results, argument); return readExecutionJob(results, argument);
  }
  if (command === 'list' && !argument) return listExecutionJobs(results,
    { after: values.after, limit: values.limit === undefined ? undefined : Number(values.limit) });
  if (command === 'work' && argument) {
    const host = values.host ?? env.STACK_BENCH_HOST_ID;
    if (!host) throw new Error('job work requires --host or STACK_BENCH_HOST_ID');
    const controller = new AbortController();
    const stop = () => controller.abort();
    process.on('SIGTERM', stop); process.on('SIGINT', stop);
    try { return await workExecutionJob(results, argument, host, { env, signal: controller.signal }); }
    finally { process.off('SIGTERM', stop); process.off('SIGINT', stop); }
  }
  throw new Error('use job submit <json|->, list, status <id>, cancel <id>, or work <id> --host <host>');
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  jobCommand(process.argv.slice(2)).then(result => {
    console.log(JSON.stringify(result, null, 2));
    if ('status' in result && result.status === 'failed') process.exitCode = 1;
  }).catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : String(error)); process.exitCode = 1;
  });
}
