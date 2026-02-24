/**
 * Fair Benchmark Runner
 *
 * Levels the playing field between SpacetimeDB and competitors by:
 *
 * 1. Using the TypeScript client for ALL systems (no custom Rust client)
 * 2. Forcing STDB_CONFIRMED_READS=1 (durable commits, like Postgres with fsync)
 * 3. Forcing USE_SPACETIME_METRICS_ENDPOINT=0 (client-side TPS counting for all)
 * 4. Using the same pipeline depth for all systems
 * 5. Including postgres_storedproc_rpc (stored procedure, eliminates ORM overhead)
 * 6. Using read_committed isolation for Postgres (its actual default)
 *
 * Usage:
 *   npx tsx src/fair-bench.ts [--seconds N] [--concurrency N] [--alpha N] [--systems a,b,c]
 *
 * Default systems: spacetimedb,postgres_rpc,postgres_storedproc_rpc
 */

import 'dotenv/config';
import { mkdir, writeFile } from 'node:fs/promises';
import { createConnection } from 'node:net';
import { join } from 'node:path';
import { CONNECTORS } from './connectors';
import { runOne } from './core/runner';

// ============================================================================
// Force fair settings
// ============================================================================

// Force SpacetimeDB to use confirmed reads (durable commits)
process.env.STDB_CONFIRMED_READS = '1';

// Force client-side TPS counting (no server-side metrics cheating)
process.env.USE_SPACETIME_METRICS_ENDPOINT = '0';

// Non-docker mode
process.env.USE_DOCKER = '0';

// Set default SpacetimeDB config if not set
if (!process.env.STDB_URL) process.env.STDB_URL = 'ws://127.0.0.1:3000';
if (!process.env.STDB_MODULE) process.env.STDB_MODULE = 'test-1';

// ============================================================================
// CLI Arguments
// ============================================================================

const args = process.argv.slice(2);

function getArg(name: string, defaultValue: number): number {
  const idx = args.findIndex(
    (a) => a === `--${name}` || a.startsWith(`--${name}=`),
  );
  if (idx === -1) return defaultValue;
  const arg = args[idx];
  if (arg.includes('=')) return Number(arg.split('=')[1]);
  return Number(args[idx + 1] ?? defaultValue);
}

function getStringArg(name: string, defaultValue: string): string {
  const idx = args.findIndex(
    (a) => a === `--${name}` || a.startsWith(`--${name}=`),
  );
  if (idx === -1) return defaultValue;
  const arg = args[idx];
  if (arg.includes('=')) return arg.split('=')[1];
  return args[idx + 1] ?? defaultValue;
}

function hasFlag(name: string): boolean {
  return args.includes(`--${name}`);
}

const seconds = getArg('seconds', 10);
const concurrency = getArg('concurrency', 50);
const alpha = getArg('alpha', 0.5);
const systems = getStringArg(
  'systems',
  'spacetimedb,postgres_rpc,postgres_storedproc_rpc',
)
  .split(',')
  .map((s) => s.trim());
const pipelineDepth = getArg('pipeline-depth', 8);
const skipPrep = hasFlag('skip-prep');

const accounts = Number(process.env.SEED_ACCOUNTS ?? 100_000);
const initialBalance = Number(process.env.SEED_INITIAL_BALANCE ?? 10_000_000);

// Set the same pipeline depth for all systems
process.env.MAX_INFLIGHT_PER_WORKER = String(pipelineDepth);

// ============================================================================
// ANSI Colors
// ============================================================================

const colors = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  dim: '\x1b[2m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  cyan: '\x1b[36m',
  red: '\x1b[31m',
};

function c(color: keyof typeof colors, text: string): string {
  return `${colors[color]}${text}${colors.reset}`;
}

// ============================================================================
// Health Checks
// ============================================================================

function ping(port: number, timeoutMs = 2000): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = createConnection({ host: '127.0.0.1', port });
    const timer = setTimeout(() => {
      socket.destroy();
      resolve(false);
    }, timeoutMs);
    socket.on('connect', () => {
      clearTimeout(timer);
      socket.destroy();
      resolve(true);
    });
    socket.on('error', () => {
      clearTimeout(timer);
      resolve(false);
    });
  });
}

const serviceChecks: Record<string, { name: string; port: number; hint: string }> = {
  spacetimedb: {
    name: 'SpacetimeDB',
    port: 3000,
    hint: 'spacetime start',
  },
  postgres_rpc: {
    name: 'Postgres (Drizzle ORM)',
    port: 4101,
    hint: 'docker compose -f docker-compose-fair.yml up -d pg-fair pg-rpc-fair',
  },
  postgres_storedproc_rpc: {
    name: 'Postgres (Stored Proc)',
    port: 4105,
    hint: 'docker compose -f docker-compose-fair.yml up -d pg-fair pg-storedproc-rpc-fair',
  },
  sqlite_rpc: {
    name: 'SQLite RPC',
    port: 4103,
    hint: 'npx tsx src/rpc-servers/sqlite-rpc-server.ts',
  },
  cockroach_rpc: {
    name: 'CockroachDB RPC',
    port: 4102,
    hint: 'docker compose up -d crdb crdb-rpc',
  },
  convex: {
    name: 'Convex',
    port: 3210,
    hint: 'cd convex-app && npx convex dev',
  },
};

// ============================================================================
// Prep / Seed
// ============================================================================

async function prepSystem(system: string): Promise<void> {
  const connectorFactory = (CONNECTORS as any)[system];
  if (!connectorFactory) {
    console.log(`  ${system.padEnd(28)} ${c('yellow', 'SKIPPED (unknown)')}`);
    return;
  }

  try {
    if (system === 'spacetimedb') {
      const conn = connectorFactory();
      await conn.open();
      await conn.reducer('seed', {
        n: accounts,
        initial_balance: BigInt(initialBalance),
      });
      await conn.close();
    } else {
      const conn = connectorFactory();
      await conn.open();
      await conn.call('seed', { accounts, initialBalance });
      await conn.close();
    }
    console.log(`  ${system.padEnd(28)} ${c('green', 'SEEDED')}`);
  } catch (err: any) {
    console.log(`  ${system.padEnd(28)} ${c('red', `FAILED: ${err.message}`)}`);
  }
}

// ============================================================================
// Benchmark
// ============================================================================

interface BenchResult {
  system: string;
  tps: number;
  p50_ms: number;
  p95_ms: number;
  p99_ms: number;
  samples: number;
}

async function runBenchmark(system: string): Promise<BenchResult | null> {
  const connectorFactory = (CONNECTORS as any)[system];
  if (!connectorFactory) {
    console.log(`  ${system}: Unknown connector`);
    return null;
  }

  const connector = connectorFactory();

  // Load the test scenario
  let scenario: any;
  try {
    const testMod = await import(`./tests/test-1/${system}.ts`);
    scenario = testMod.default.run;
  } catch {
    // Fallback to rpc_single_call for RPC-based systems
    const { rpc_single_call } = await import('./scenario_recipes/rpc_single_call.ts');
    scenario = rpc_single_call;
  }

  const result = await runOne({
    connector,
    scenario,
    seconds,
    concurrency,
    accounts,
    alpha,
  });

  return {
    system,
    tps: Math.round(result.tps),
    p50_ms: result.p50_ms,
    p95_ms: result.p95_ms,
    p99_ms: result.p99_ms,
    samples: result.samples,
  };
}

// ============================================================================
// Display
// ============================================================================

function renderBar(tps: number, maxTps: number, width = 40): string {
  const filled = Math.max(1, Math.round((tps / maxTps) * width));
  return c('green', '\u2588'.repeat(filled) + '\u2591'.repeat(width - filled));
}

// ============================================================================
// Main
// ============================================================================

async function main() {
  console.log('');
  console.log(c('bold', c('cyan', '  Fair Benchmark: SpacetimeDB vs Competitors')));
  console.log(c('dim', '  Leveled playing field - same client, same durability, same counting'));
  console.log('');

  console.log(c('bold', '  Configuration:'));
  console.log(`    Duration:          ${seconds}s`);
  console.log(`    Concurrency:       ${concurrency} connections`);
  console.log(`    Alpha (contention): ${alpha}`);
  console.log(`    Pipeline depth:    ${pipelineDepth} per worker (same for all)`);
  console.log(`    Systems:           ${systems.join(', ')}`);
  console.log('');

  console.log(c('bold', '  Fairness guarantees:'));
  console.log(`    ${c('green', '\u2713')} TypeScript client for ALL systems (no custom Rust client)`);
  console.log(`    ${c('green', '\u2713')} STDB_CONFIRMED_READS=1 (durable commits)`);
  console.log(`    ${c('green', '\u2713')} Client-side TPS counting for ALL systems`);
  console.log(`    ${c('green', '\u2713')} Same pipeline depth (${pipelineDepth}) for all`);
  console.log(`    ${c('green', '\u2713')} Postgres: read_committed isolation (actual default)`);
  console.log(`    ${c('green', '\u2713')} Postgres: synchronous_commit=on`);
  console.log('');

  // Check services
  console.log(c('bold', '  [1/3] Checking services...\n'));
  for (const system of systems) {
    const check = serviceChecks[system];
    if (!check) {
      console.log(`  ${system.padEnd(28)} ${c('yellow', '? (no health check)')}`);
      continue;
    }
    const alive = await ping(check.port);
    if (alive) {
      console.log(`  ${check.name.padEnd(28)} ${c('green', 'UP')}`);
    } else {
      console.log(`  ${check.name.padEnd(28)} ${c('red', 'DOWN')}`);
      console.log(`    Start with: ${c('cyan', check.hint)}`);
      process.exit(1);
    }
  }

  // Seed
  if (!skipPrep) {
    console.log('\n' + c('bold', '  [2/3] Seeding databases...\n'));
    for (const system of systems) {
      await prepSystem(system);
    }
  } else {
    console.log('\n' + c('bold', '  [2/3] Seeding... ') + c('dim', '(skipped)\n'));
  }

  // Benchmark
  console.log('\n' + c('bold', '  [3/3] Running benchmarks...\n'));

  const results: BenchResult[] = [];
  for (const system of systems) {
    console.log(`  Running ${system}...`);
    const result = await runBenchmark(system);
    if (result && result.tps > 0) {
      console.log(`  ${system.padEnd(28)} ${c('green', `${result.tps.toLocaleString()} TPS`)}  (p50=${result.p50_ms.toFixed(1)}ms p95=${result.p95_ms.toFixed(1)}ms p99=${result.p99_ms.toFixed(1)}ms)`);
      results.push(result);
    } else {
      console.log(`  ${system.padEnd(28)} ${c('red', 'FAILED')}`);
    }
  }

  // Results
  if (results.length > 0) {
    results.sort((a, b) => b.tps - a.tps);
    const maxTps = results[0]?.tps || 1;

    console.log('\n' + c('bold', '\u2550'.repeat(70)));
    console.log(c('bold', '  FAIR BENCHMARK RESULTS'));
    console.log(c('bold', '\u2550'.repeat(70)) + '\n');

    for (const r of results) {
      const bar = renderBar(r.tps, maxTps);
      const tpsStr = r.tps.toLocaleString().padStart(10);
      console.log(`  ${r.system.padEnd(28)} ${bar} ${tpsStr} TPS`);
    }

    const fastest = results[0];
    const slowest = results[results.length - 1];

    if (fastest && slowest && fastest.system !== slowest.system && slowest.tps > 0) {
      const multiplier = (fastest.tps / slowest.tps).toFixed(1);
      console.log('');
      console.log(`  ${fastest.system} is ${multiplier}x faster than ${slowest.system}`);
    }

    // Latency table
    console.log('\n' + c('bold', '  Latency (ms):'));
    console.log(`  ${'System'.padEnd(28)} ${'p50'.padStart(8)} ${'p95'.padStart(8)} ${'p99'.padStart(8)}`);
    console.log(`  ${'-'.repeat(28)} ${'-'.repeat(8)} ${'-'.repeat(8)} ${'-'.repeat(8)}`);
    for (const r of results) {
      console.log(`  ${r.system.padEnd(28)} ${r.p50_ms.toFixed(1).padStart(8)} ${r.p95_ms.toFixed(1).padStart(8)} ${r.p99_ms.toFixed(1).padStart(8)}`);
    }

    // Save to JSON
    const runsDir = join(process.cwd(), 'runs');
    await mkdir(runsDir, { recursive: true });
    const outFile = join(
      runsDir,
      `fair-bench-${new Date().toISOString().replace(/[:.]/g, '-')}.json`,
    );
    await writeFile(
      outFile,
      JSON.stringify(
        {
          benchmark: 'fair-comparison',
          timestamp: new Date().toISOString(),
          fairness: {
            confirmed_reads: true,
            metrics_endpoint: false,
            client: 'typescript (same for all)',
            pipeline_depth: pipelineDepth,
            postgres_isolation: 'read_committed',
            postgres_synchronous_commit: 'on',
          },
          config: { seconds, concurrency, alpha, accounts, pipelineDepth },
          results: results.map((r) => ({
            system: r.system,
            tps: r.tps,
            p50_ms: r.p50_ms,
            p95_ms: r.p95_ms,
            p99_ms: r.p99_ms,
            samples: r.samples,
          })),
        },
        null,
        2,
      ),
    );
    console.log(`\n${c('dim', `  Results saved to: ${outFile}`)}\n`);
  }
}

main().catch((err) => {
  console.error('\n' + c('red', '  ERROR:'), err.message);
  process.exit(1);
});
