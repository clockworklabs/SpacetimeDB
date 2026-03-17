import 'dotenv/config';
import { execSync } from 'node:child_process';
import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { createConnection } from 'node:net';
import { join } from 'node:path';
import { ConnectorKey, CONNECTORS } from './connectors';
import { runOne } from './core/runner';
import { initConvex } from './init/init_convex';
import { sh } from './init/utils';
import cac from 'cac';

// Simple TCP ping - just check if something is listening on the port
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

// Use spacetime CLI to ping the server
function spacetimePing(): boolean {
  try {
    execSync('spacetime server ping local', { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

// ============================================================================
// CLI Arguments
// ============================================================================

const parser = cac()
  .option('--seconds <seconds>', 'Number of seconds to benchmark for', {
    default: 10,
  })
  .option('--concurrency <concurrency>', 'Concurrent clients to run', {
    default: 10,
  })
  .option('--alpha <alpha>', 'Alpha value', { default: 1.5 })
  .option(
    '--systems <systems>',
    `The systems to run against (valid values: ${Object.keys(CONNECTORS).join(', ')})`,
    {
      default: 'convex,spacetimedb',
      type: [
        (s: string | string[]) =>
          (Array.isArray(s) ? s : s.split(',')).map((s) => {
            const x = s.trim();
            if (!Object.prototype.hasOwnProperty.call(CONNECTORS, x)) {
              throw new Error(`${x} is not a valid system`);
            }
            return x;
          }),
      ],
    },
  )
  .option('--skip-prep', 'Skip prep')
  .option('--no-animation', 'No animation')
  .help()
  .usage('[options]');

const args = parser.parse();

parser.globalCommand.checkUnknownOptions();
parser.globalCommand.checkOptionValue();
parser.globalCommand.checkRequiredArgs();
parser.globalCommand.checkUnusedArgs();

if (args.options.help) {
  process.exit(0);
}

const seconds = Number(args.options.seconds);
const concurrency = Number(args.options.concurrency);
const alpha = Number(args.options.alpha);
const systems: ConnectorKey[] = args.options.systems.flat();
const skipPrep: boolean = args.options.skipPrep;
const noAnimation: boolean = !args.options.animation;

const accounts = Number(process.env.SEED_ACCOUNTS ?? 100_000);
const initialBalance = Number(process.env.SEED_INITIAL_BALANCE ?? 10_000_000);

// Force non-Docker mode and use metrics endpoint for TPS counting
process.env.USE_DOCKER = '0';
process.env.USE_SPACETIME_METRICS_ENDPOINT = '1';

// Set default SpacetimeDB config if not set
if (!process.env.STDB_URL) process.env.STDB_URL = 'ws://127.0.0.1:3000';
if (!process.env.STDB_MODULE) process.env.STDB_MODULE = 'test-1';

// ============================================================================
// ANSI Colors & Animation
// ============================================================================

const colors = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  dim: '\x1b[2m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  red: '\x1b[31m',
};

function c(color: keyof typeof colors, text: string): string {
  if (noAnimation) return text;
  return `${colors[color]}${text}${colors.reset}`;
}

const spinnerFrames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

function createSpinner(label: string): { stop: (finalText: string) => void } {
  if (noAnimation) {
    process.stdout.write(`  ${label}...`);
    return {
      stop: (finalText: string) => {
        console.log(` ${finalText}`);
      },
    };
  }

  let frame = 0;
  const interval = setInterval(() => {
    process.stdout.write(
      `\r  ${spinnerFrames[frame++ % spinnerFrames.length]} ${label}...`,
    );
  }, 80);

  return {
    stop: (finalText: string) => {
      clearInterval(interval);
      process.stdout.write(`\r  ${label}... ${finalText}          \n`);
    },
  };
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// ============================================================================
// Service Health Checks
// ============================================================================

interface ServiceConfig {
  name: string;
  healthCheck: () => Promise<boolean>;
  startCmd: string;
  startCwd?: string;
}

const serviceConfigs: Record<string, ServiceConfig> = {
  spacetimedb: {
    name: 'SpacetimeDB',
    healthCheck: async () => spacetimePing(),
    startCmd: 'spacetime start',
  },
  spacetimedbRustClient: {
    name: 'SpacetimeDB',
    healthCheck: async () => spacetimePing(),
    startCmd: 'spacetime start',
  },
  convex: {
    name: 'Convex',
    healthCheck: () => ping(3210),
    startCmd: 'npx convex dev',
    startCwd: 'convex-app',
  },
  postgres_rpc: {
    name: 'Postgres RPC',
    healthCheck: () => ping(4101),
    startCmd: 'npx tsx src/rpc-servers/postgres-rpc-server.ts',
  },
  sqlite_rpc: {
    name: 'SQLite RPC',
    healthCheck: () => ping(4103),
    startCmd: 'npx tsx src/rpc-servers/sqlite-rpc-server.ts',
  },
  cockroach_rpc: {
    name: 'CockroachDB RPC',
    healthCheck: () => ping(4102),
    startCmd: 'npx tsx src/rpc-servers/cockroach-rpc-server.ts',
  },
  supabase_rpc: {
    name: 'Supabase RPC',
    healthCheck: () => ping(4106),
    startCmd: 'npx tsx src/rpc-servers/supabase-rpc-server.ts',
  },
  bun: {
    name: 'Bun',
    healthCheck: () => ping(4001),
    startCmd: 'bun run bun/bun-server.ts',
  },
};

async function checkService(system: string): Promise<boolean> {
  const config = serviceConfigs[system];
  if (!config) return true; // Unknown system, assume ready

  const isRunning = await config.healthCheck();
  if (isRunning) {
    console.log(`  ${config.name.padEnd(15)} ${c('green', '✓')}`);
    return true;
  }

  console.log(`  ${config.name.padEnd(15)} ${c('red', '✗ NOT RUNNING')}`);
  console.log(`\n  Please start ${config.name} in another terminal:`);
  console.log(`    ${c('cyan', config.startCmd)}`);
  if (config.startCwd) {
    console.log(`    ${c('dim', `(from directory: ${config.startCwd})`)}`);
  }
  console.log(`\n  Press Enter when ready...`);

  await new Promise<void>((resolve) => {
    process.stdin.once('data', () => resolve());
  });

  const nowRunning = await config.healthCheck();
  if (nowRunning) {
    console.log(`  ${config.name.padEnd(15)} ${c('green', '✓')}`);
  }
  return nowRunning;
}

// ============================================================================
// Prep / Seed
// ============================================================================

async function prepSystem(system: ConnectorKey): Promise<void> {
  const connector = CONNECTORS[system];
  if (!connector) {
    console.log(`  ${system.padEnd(15)} ${c('yellow', '⚠ SKIPPED')}`);
    return;
  }

  const spinner = createSpinner(system.padEnd(15));

  try {
    if (system === 'spacetimedb' || system == 'spacetimedbRustClient') {
      const moduleName = process.env.STDB_MODULE || 'test-1';
      const server = process.env.STDB_SERVER || 'local';
      // const server2 = process.env.STDB_SERVER || 'http://localhost:3000';
      const modulePath = process.env.STDB_MODULE_PATH || './spacetimedb';

      // Publish module (creates DB if needed, updates if exists)
      await sh('spacetime', [
        'publish',
        '-c',
        '-y',
        '--server',
        server,
        moduleName,
        '--module-path',
        modulePath,
      ]);
      await sh('spacetime', [
        'call',
        '--server',
        server,
        moduleName,
        'seed',
        String(accounts),
        String(initialBalance),
      ]);
      console.log('[spacetimedb] seed complete.');
    } else if (system === 'convex') {
      await initConvex();
    } else {
      const conn = connector();
      await conn.open();
      await conn.call('seed', { accounts, initialBalance });
      await conn.close();
    }
    spinner.stop(c('green', '✓ READY'));
  } catch (err: any) {
    spinner.stop(c('red', `✗ ${err.message}`));
  }
}

// ============================================================================
// Benchmark
// ============================================================================

interface BenchResult {
  system: string;
  tps: number;
}

async function runBenchmarkOther(
  system: ConnectorKey,
): Promise<BenchResult | null> {
  const connectorFactory = CONNECTORS[system];
  if (!connectorFactory) {
    console.log(`  ${system}: Unknown connector`);
    return null;
  }

  const connector = connectorFactory();
  const testMod = await import(`./tests/test-1/${system}.ts`);
  const scenario = testMod.default.run;

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
  };
}

async function runBenchmarkStdb(): Promise<BenchResult | null> {
  const moduleName = process.env.STDB_MODULE || 'test-1';
  const server2 = process.env.STDB_SERVER || 'http://localhost:3000';

  await sh('cargo', [
    'run',
    //"--quiet",
    '--manifest-path',
    'spacetimedb-rust-client/Cargo.toml',
    '--',
    'bench',
    //"--quiet",
    '--server',
    server2,
    '--module',
    moduleName,
    '--duration',
    `${seconds}s`,
    '--connections',
    String(concurrency),
    '--alpha',
    String(alpha),
    '--tps-write-path',
    'spacetimedb-tps.tmp.log',
  ]);

  const tpsStr = (await readFile('spacetimedb-tps.tmp.log', 'utf-8')).trim();
  const tps = Number(tpsStr);
  if (isNaN(tps)) {
    console.warn(`[spacetimedb] Failed to parse TPS from file: ${tpsStr}`);
    return null;
  }

  return {
    system: 'spacetimedb',
    tps: Math.round(tps),
  };
}

async function runBenchmark(system: ConnectorKey): Promise<BenchResult | null> {
  if (system === 'spacetimedbRustClient') {
    return await runBenchmarkStdb();
  } else {
    return await runBenchmarkOther(system);
  }
}

// ============================================================================
// Display
// ============================================================================

function renderBar(tps: number, maxTps: number, width = 40): string {
  const filled = Math.max(1, Math.round((tps / maxTps) * width));
  const bar = '█'.repeat(filled) + '░'.repeat(width - filled);
  return c('green', bar);
}

async function displayResults(results: BenchResult[]): Promise<void> {
  results.sort((a, b) => b.tps - a.tps);
  const maxTps = results[0]?.tps || 1;

  console.log('\n' + c('bold', '═'.repeat(70)));
  console.log(c('bold', '  RESULTS'));
  console.log(c('bold', '═'.repeat(70)) + '\n');

  if (noAnimation) {
    // Static display
    for (const r of results) {
      const bar = renderBar(r.tps, maxTps);
      const tpsStr = r.tps.toLocaleString().padStart(10);
      console.log(`  ${r.system.padEnd(14)} ${bar} ${tpsStr} TPS`);
    }
  } else {
    // Animated bars growing
    const frames = 25;
    for (let i = 1; i <= frames; i++) {
      const progress = i / frames;

      // Move cursor up to redraw (except first frame)
      if (i > 1) {
        process.stdout.write(`\x1b[${results.length}A`);
      }

      for (const r of results) {
        const currentTps = Math.round(r.tps * progress);
        const bar = renderBar(currentTps, maxTps);
        const tpsStr = currentTps.toLocaleString().padStart(10);
        console.log(`  ${r.system.padEnd(14)} ${bar} ${tpsStr} TPS`);
      }

      await sleep(40);
    }
  }

  // Show comparison
  const fastest = results[0];
  const slowest = results[results.length - 1];

  if (
    fastest &&
    slowest &&
    fastest.system !== slowest.system &&
    slowest.tps > 0
  ) {
    const multiplier = Math.round(fastest.tps / slowest.tps);

    console.log('');

    if (!noAnimation) {
      // Animated reveal of the comparison box
      await sleep(200);
    }

    const boxWidth = 60;
    const msgText = `${fastest.system} is ${multiplier}x FASTER than ${slowest.system}!`;
    const msgWithEmoji = `🚀 ${msgText} 🚀`;
    // Emojis are 2 display columns each, so total display width = text + 4 (2 emojis) + 2 (spaces)
    const displayWidth = msgText.length + 6;
    const msgPadding = Math.floor((boxWidth - displayWidth) / 2);
    const rightPadding = boxWidth - msgPadding - displayWidth;

    console.log('  ' + c('cyan', '╔' + '═'.repeat(boxWidth) + '╗'));
    console.log('  ' + c('cyan', '║') + ' '.repeat(boxWidth) + c('cyan', '║'));
    console.log(
      '  ' +
        c('cyan', '║') +
        ' '.repeat(msgPadding) +
        c('bold', c('green', msgWithEmoji)) +
        ' '.repeat(rightPadding) +
        c('cyan', '║'),
    );
    console.log('  ' + c('cyan', '║') + ' '.repeat(boxWidth) + c('cyan', '║'));
    console.log('  ' + c('cyan', '╚' + '═'.repeat(boxWidth) + '╝'));
  }
}

// ============================================================================
// Main
// ============================================================================

async function main() {
  const headerWidth = 59;
  const headerText = 'SpacetimeDB Benchmark Demo';
  const headerPadding = Math.floor((headerWidth - headerText.length) / 2);
  const headerPaddedText =
    ' '.repeat(headerPadding) +
    headerText +
    ' '.repeat(headerWidth - headerPadding - headerText.length);

  console.log('');
  console.log(c('bold', c('cyan', '  ╔' + '═'.repeat(headerWidth) + '╗')));
  console.log(c('bold', c('cyan', '  ║') + headerPaddedText + c('cyan', '║')));
  console.log(c('bold', c('cyan', '  ╚' + '═'.repeat(headerWidth) + '╝')));
  console.log('');

  console.log(
    `  ${c('dim', 'Config:')} ${seconds}s, ${concurrency} connections, alpha=${alpha}`,
  );
  console.log(`  ${c('dim', 'Systems:')} ${systems.join(', ')}\n`);

  // Step 1: Check services
  console.log(c('bold', '  [1/4] Checking services...\n'));

  for (const system of systems) {
    const ok = await checkService(system);
    if (!ok) {
      console.log(
        `\n${c('red', '  ERROR:')} ${system} is not running. Exiting.`,
      );
      process.exit(1);
    }
  }

  // Step 2: Prep/seed
  if (!skipPrep) {
    console.log('\n' + c('bold', '  [2/4] Preparing databases...\n'));
    for (const system of systems) {
      await prepSystem(system);
    }
  } else {
    console.log(
      '\n' +
        c('bold', '  [2/4] Preparing databases...') +
        c('dim', ' (skipped)\n'),
    );
  }

  // Step 3: Run benchmarks
  console.log('\n' + c('bold', '  [3/4] Running benchmarks...\n'));

  const results: BenchResult[] = [];
  for (const system of systems) {
    const spinner = createSpinner(`${system.padEnd(12)} benchmarking`);
    const result = await runBenchmark(system);
    if (result && result.tps > 0) {
      spinner.stop(c('green', `✓ ${result.tps.toLocaleString()} TPS`));
      results.push(result);
    } else {
      spinner.stop(c('red', `✗ FAILED (0 completed transactions)`));
    }
  }

  // Step 4: Display results
  if (results.length > 0) {
    await displayResults(results);

    // Save to JSON
    const runsDir = join(process.cwd(), 'runs');
    await mkdir(runsDir, { recursive: true });
    const outFile = join(
      runsDir,
      `demo-${new Date().toISOString().replace(/[:.]/g, '-')}.json`,
    );
    await writeFile(
      outFile,
      JSON.stringify(
        {
          timestamp: new Date().toISOString(),
          config: { seconds, concurrency, alpha, accounts },
          results: results.map((r) => ({
            system: r.system,
            tps: r.tps,
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
