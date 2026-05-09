/**
 * Fair Benchmark Runner
 *
 * Levels the playing field between SpacetimeDB and competitors by:
 *
 * 1. Same TypeScript client for ALL systems (SpacetimeDB Rust client was
 *    already removed from keynote-2 in master via #4753).
 * 2. STDB_CONFIRMED_READS=1 (durable commits, like Postgres with fsync).
 *    Confirmed reads is now the default in master (#4682) — kept here for
 *    belt-and-suspenders.
 * 3. Sequential / non-pipelined operations for ALL systems
 *    (BENCH_PIPELINED=0). The master default pipelines SpacetimeDB up to
 *    its `maxInflightPerWorker` while competitors' connectors don't set
 *    that field — disabling pipelining everywhere keeps per-conn concurrency
 *    even.
 * 4. Includes postgres_storedproc_rpc by default — same architecture as
 *    SpacetimeDB's reducer (single atomic DB call) instead of 5 ORM
 *    round-trips via Drizzle.
 * 5. Postgres uses read_committed isolation + synchronous_commit=on
 *    (configured in docker-compose-fair.yml).
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
import { CONNECTORS, type ConnectorKey } from './connectors';
import { runOne } from './core/runner';
import { parseBenchOptions } from './opts.ts';
import type { BaseConnector } from './core/connectors.ts';

type Scenario = (
  conn: BaseConnector,
  from: number,
  to: number,
  amount: number,
) => Promise<void>;

// Force fair settings via env BEFORE parsing options, so that
// getSharedRuntimeDefaults picks them up.
process.env.STDB_CONFIRMED_READS = '1';
process.env.BENCH_PIPELINED = '0';
if (!process.env.USE_DOCKER) process.env.USE_DOCKER = '0';
if (!process.env.STDB_URL) process.env.STDB_URL = 'ws://127.0.0.1:3000';
if (!process.env.STDB_MODULE) process.env.STDB_MODULE = 'test-1';

// Reuse the standard bench CLI parser so users get the full option set,
// then override fairness-relevant options if the user overrode them.
// Strip fair-bench-specific flags before delegating to the bench parser,
// which would otherwise reject them as unknown.
const skipPrep = process.argv.includes('--skip-prep');
const benchArgv = process.argv.filter((arg) => arg !== '--skip-prep');
const options = parseBenchOptions(benchArgv);

// Default systems if --connectors / --systems wasn't passed
const FAIR_DEFAULT_SYSTEMS: readonly ConnectorKey[] = [
  'spacetimedb',
  'postgres_rpc',
  'postgres_storedproc_rpc',
];

const requestedConnectors: ConnectorKey[] = options.connectors ?? [
  ...FAIR_DEFAULT_SYSTEMS,
];

// Force non-pipelined regardless of what was parsed (env above already
// nudged this, but the user could pass --bench-pipelined on the CLI; we
// silently override to keep "fair").
const fairOptions = {
  ...options,
  benchPipelined: false,
  stdbConfirmedReads: true,
};

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

const serviceChecks: Partial<Record<
  ConnectorKey,
  { name: string; port: number; hint: string }
>> = {
  spacetimedb: {
    name: 'SpacetimeDB',
    port: Number(process.env.STDB_PORT ?? 3000),
    hint: 'docker compose -f docker-compose-fair.yml up -d spacetime-fair',
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

async function prepSystem(system: ConnectorKey): Promise<void> {
  const factory = CONNECTORS[system];
  if (!factory) {
    console.log(`  ${system.padEnd(28)} ${c('yellow', 'SKIPPED (unknown)')}`);
    return;
  }

  const conn = factory(fairOptions) as BaseConnector & {
    call?: (name: string, args?: Record<string, unknown>) => Promise<unknown>;
  };

  try {
    await conn.open();
    if (typeof conn.call === 'function') {
      await conn.call('seed', {
        accounts: fairOptions.accounts,
        initialBalance: fairOptions.initialBalance,
      });
    }
    await conn.close();
    console.log(`  ${system.padEnd(28)} ${c('green', 'SEEDED')}`);
  } catch (err: any) {
    console.log(
      `  ${system.padEnd(28)} ${c('red', `FAILED: ${err?.message ?? err}`)}`,
    );
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

async function runBenchmark(system: ConnectorKey): Promise<BenchResult | null> {
  const factory = CONNECTORS[system];
  if (!factory) {
    console.log(`  ${system}: Unknown connector`);
    return null;
  }

  const connector = factory(fairOptions);

  // Pick the right scenario for the connector kind. RPC connectors and
  // SpacetimeDB share a `.call(name, args)` interface, so both work with
  // either recipe — but we prefer the system-specific test if registered.
  let scenario: Scenario;
  try {
    const testMod = await import(`./tests/test-1/${system}.ts`);
    scenario = testMod.default.run as Scenario;
  } catch (err: any) {
    if (
      err?.code === 'ERR_MODULE_NOT_FOUND' ||
      err?.code === 'MODULE_NOT_FOUND'
    ) {
      const fallbackPath =
        system === 'spacetimedb'
          ? './scenario_recipes/reducer_single.ts'
          : './scenario_recipes/rpc_single_call.ts';
      const recipeMod = await import(fallbackPath);
      scenario = (recipeMod.reducer_single ??
        recipeMod.rpc_single_call) as Scenario;
    } else {
      throw err;
    }
  }

  const result = await runOne({
    connector,
    scenario,
    seconds: fairOptions.seconds,
    concurrency: fairOptions.concurrency,
    accounts: fairOptions.accounts,
    alpha: fairOptions.alpha,
    runtimeConfig: fairOptions,
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
  return c('green', '█'.repeat(filled) + '░'.repeat(width - filled));
}

// ============================================================================
// Main
// ============================================================================

async function main() {
  console.log('');
  console.log(c('bold', c('cyan', '  Fair Benchmark: SpacetimeDB vs Competitors')));
  console.log(c('dim', '  Leveled playing field - same client, same durability, same pipelining'));
  console.log('');

  console.log(c('bold', '  Configuration:'));
  console.log(`    Duration:          ${fairOptions.seconds}s`);
  console.log(`    Concurrency:       ${fairOptions.concurrency} connections`);
  console.log(`    Alpha (contention): ${fairOptions.alpha}`);
  console.log(`    Systems:           ${requestedConnectors.join(', ')}`);
  console.log('');

  console.log(c('bold', '  Fairness guarantees:'));
  console.log(`    ${c('green', '✓')} TypeScript client for ALL systems`);
  console.log(`    ${c('green', '✓')} STDB_CONFIRMED_READS=1 (durable commits)`);
  console.log(`    ${c('green', '✓')} Sequential (non-pipelined) operations for all systems`);
  console.log(`    ${c('green', '✓')} Postgres: read_committed isolation`);
  console.log(`    ${c('green', '✓')} Postgres: synchronous_commit=on`);
  console.log('');

  // Check services
  console.log(c('bold', '  [1/3] Checking services...\n'));
  for (const system of requestedConnectors) {
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
    for (const system of requestedConnectors) {
      await prepSystem(system);
    }
  } else {
    console.log('\n' + c('bold', '  [2/3] Seeding... ') + c('dim', '(skipped)\n'));
  }

  // Benchmark
  console.log('\n' + c('bold', '  [3/3] Running benchmarks...\n'));

  const results: BenchResult[] = [];
  for (const system of requestedConnectors) {
    console.log(`  Running ${system}...`);
    try {
      const result = await runBenchmark(system);
      if (result && result.tps > 0) {
        console.log(
          `  ${system.padEnd(28)} ${c('green', `${result.tps.toLocaleString()} TPS`)}  (p50=${result.p50_ms.toFixed(1)}ms p95=${result.p95_ms.toFixed(1)}ms p99=${result.p99_ms.toFixed(1)}ms)`,
        );
        results.push(result);
      } else {
        console.log(`  ${system.padEnd(28)} ${c('red', 'FAILED')}`);
      }
    } catch (err: any) {
      console.log(
        `  ${system.padEnd(28)} ${c('red', `FAILED: ${err?.message ?? err}`)}`,
      );
    }
  }

  // Results
  if (results.length > 0) {
    results.sort((a, b) => b.tps - a.tps);
    const maxTps = results[0]?.tps || 1;

    console.log('\n' + c('bold', '═'.repeat(70)));
    console.log(c('bold', '  FAIR BENCHMARK RESULTS'));
    console.log(c('bold', '═'.repeat(70)) + '\n');

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
            client: 'typescript (same for all)',
            pipelined: false,
            postgres_isolation: 'read_committed',
            postgres_synchronous_commit: 'on',
          },
          config: {
            seconds: fairOptions.seconds,
            concurrency: fairOptions.concurrency,
            alpha: fairOptions.alpha,
            accounts: fairOptions.accounts,
          },
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
