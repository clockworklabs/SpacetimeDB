// CLI entry point for the perf benchmark.
//
// Usage:
//   tsx src/main.ts --backend pg --scenario stress      [--writers 50] [--duration 60]
//   tsx src/main.ts --backend stdb --scenario realistic [--users 100] [--duration 120]
//   tsx src/main.ts --backend pg --scenario soak        [--cap 1000] [--ramp 20]
//   tsx src/main.ts --backend stdb --scenario all
//
// PG defaults: http://localhost:6001
// STDB defaults: ws://localhost:3000, module from --module flag

import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  runStressPostgres,
  runStressSpacetime,
  type StressOpts,
} from './scenarios/stress-throughput.ts';
import {
  runRealisticPostgres,
  runRealisticSpacetime,
  type RealisticOpts,
} from './scenarios/realistic-chat.ts';
import {
  runSoakPostgres,
  runSoakSpacetime,
  type SoakOpts,
} from './scenarios/connection-soak.ts';
import type { ScenarioResult } from './metrics.ts';

interface CliArgs {
  backend: 'pg' | 'stdb';
  scenario: 'stress' | 'realistic' | 'soak' | 'all';
  pgUrl: string;
  stdbUri: string;
  stdbModule: string;
  writers: number;
  users: number;
  duration: number;
  cap: number;
  ramp: number;
  out: string;
}

function parseArgs(argv: string[]): CliArgs {
  const a: CliArgs = {
    backend: 'pg',
    scenario: 'stress',
    pgUrl: 'http://localhost:6001',
    stdbUri: 'ws://localhost:3000',
    stdbModule: '',
    writers: 20,
    users: 50,
    duration: 30,
    cap: 500,
    ramp: 20,
    out: '',
  };
  for (let i = 0; i < argv.length; i++) {
    const k = argv[i];
    const v = argv[i + 1];
    switch (k) {
      case '--backend': a.backend = v as 'pg' | 'stdb'; i++; break;
      case '--scenario': a.scenario = v as CliArgs['scenario']; i++; break;
      case '--pg-url': a.pgUrl = v!; i++; break;
      case '--stdb-uri': a.stdbUri = v!; i++; break;
      case '--module': a.stdbModule = v!; i++; break;
      case '--writers': a.writers = parseInt(v!); i++; break;
      case '--users': a.users = parseInt(v!); i++; break;
      case '--duration': a.duration = parseInt(v!); i++; break;
      case '--cap': a.cap = parseInt(v!); i++; break;
      case '--ramp': a.ramp = parseInt(v!); i++; break;
      case '--out': a.out = v!; i++; break;
    }
  }
  return a;
}

async function runOne(args: CliArgs, scenario: 'stress' | 'realistic' | 'soak'): Promise<ScenarioResult> {
  if (args.backend === 'pg') {
    const cfg = { baseUrl: args.pgUrl };
    if (scenario === 'stress') return runStressPostgres(cfg, { writers: args.writers, durationSec: args.duration });
    if (scenario === 'realistic') return runRealisticPostgres(cfg, { users: args.users, durationSec: args.duration, minIntervalMs: 5000, maxIntervalMs: 15000 });
    return runSoakPostgres(cfg, { cap: args.cap, rampPerSec: args.ramp });
  } else {
    if (!args.stdbModule) throw new Error('--module is required for stdb');
    const cfg = { uri: args.stdbUri, moduleName: args.stdbModule };
    if (scenario === 'stress') return runStressSpacetime(cfg, { writers: args.writers, durationSec: args.duration });
    if (scenario === 'realistic') return runRealisticSpacetime(cfg, { users: args.users, durationSec: args.duration, minIntervalMs: 5000, maxIntervalMs: 15000 });
    return runSoakSpacetime(cfg, { cap: args.cap, rampPerSec: args.ramp });
  }
}

function summarize(r: ScenarioResult): string {
  const ack = r.ackLatencyMs;
  const fan = r.fanoutLatencyMs;
  return [
    `[${r.backend}] ${r.scenario}: ${r.received}/${r.sent} msgs in ${r.durationSec}s`,
    `  throughput: ${r.msgsPerSec.toFixed(1)} msgs/sec`,
    `  ack       p50=${ack.p50.toFixed(1)}ms p99=${ack.p99.toFixed(1)}ms (n=${ack.count})`,
    `  fanout    p50=${fan.p50.toFixed(1)}ms p99=${fan.p99.toFixed(1)}ms (n=${fan.count})`,
    r.notes ? `  note: ${r.notes}` : '',
  ].filter(Boolean).join('\n');
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  const __dirname = dirname(fileURLToPath(import.meta.url));
  const stamp = new Date().toISOString().replace(/[:.]/g, '-');
  const outDir = args.out || join(__dirname, '..', 'results', stamp);
  mkdirSync(outDir, { recursive: true });

  const scenarios: Array<'stress' | 'realistic' | 'soak'> =
    args.scenario === 'all' ? ['stress', 'realistic', 'soak'] : [args.scenario];

  const results: ScenarioResult[] = [];
  for (const sc of scenarios) {
    console.log(`\n=== ${args.backend} / ${sc} ===`);
    try {
      const r = await runOne(args, sc);
      results.push(r);
      console.log(summarize(r));
      writeFileSync(
        join(outDir, `${args.backend}-${sc}.json`),
        JSON.stringify(r, (_k, v) => (typeof v === 'bigint' ? v.toString() : v), 2),
      );
    } catch (err) {
      console.error(`FAILED ${args.backend}/${sc}:`, err);
    }
  }

  console.log(`\nResults written to ${outDir}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
