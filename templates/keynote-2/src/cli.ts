import 'dotenv/config';
import { readdir, mkdir, writeFile } from 'node:fs/promises';
import { CONNECTORS } from './connectors';
import { runOne } from './core/runner';
import type { TestCaseModule } from './tests/types';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { RunResult } from './core/types.ts';

const args = process.argv.slice(2);

let testName = 'test-1';
let posArgs = args;

if (args.length > 0 && !args[0].startsWith('--')) {
  testName = args[0];
  posArgs = args.slice(1);
}

let seconds = 1,
  concurrency = 10,
  accounts = process.env.SEED_ACCOUNTS
    ? Number(process.env.SEED_ACCOUNTS)
    : 100_000,
  alpha = 0.5,
  connectors: string[] | null = null,
  contentionTests: {
    startAlpha: number;
    endAlpha: number;
    step: number;
    concurrency: number;
  } | null = null,
  concurrencyTests: {
    startConc: number;
    endConc: number;
    step: number;
    alpha: number;
  } | null = null;

for (let i = 0; i < posArgs.length; ) {
  const arg = posArgs[i];
  if (!arg.startsWith('--')) {
    i++;
    continue;
  }
  const key = arg.slice(2);
  const val = posArgs[i + 1];
  if (!val || val.startsWith('--')) {
    i++;
    continue;
  }

  switch (key) {
    case 'seconds':
      seconds = Number(val);
      i += 2;
      break;
    case 'concurrency':
      concurrency = Number(val);
      i += 2;
      break;
    case 'alpha':
      alpha = Number(val);
      i += 2;
      break;
    case 'connectors':
      connectors = val
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean);
      i += 2;
      break;
    case 'contention-tests':
      contentionTests = {
        startAlpha: Number(posArgs[i + 1]),
        endAlpha: Number(posArgs[i + 2]),
        step: Number(posArgs[i + 3]),
        concurrency: Number(posArgs[i + 4]),
      };
      concurrency = Number(posArgs[i + 4]);

      i += 5;
      break;
    case 'concurrency-tests':
      concurrencyTests = {
        startConc: Number(posArgs[i + 1]),
        endConc: Number(posArgs[i + 2]),
        step: Number(posArgs[i + 3]),
        alpha: Number(posArgs[i + 4]),
      };
      alpha = Number(posArgs[i + 4]);

      i += 5;
      break;
  }
}

interface BenchmarkConfig {
  connector: any;
  scenario: any;
  seconds: number;
  accounts: number;
}

class BenchmarkTester {
  private config: BenchmarkConfig;

  constructor(config: BenchmarkConfig) {
    this.config = config;
  }

  private async runAvg(
    concurrency: number,
    alpha: number,
    runs: number = 3,
  ): Promise<RunResult> {
    let totals = {
      tps: 0,
      samples: 0,
      committed_txns: 0,
      p50_ms: 0,
      p95_ms: 0,
      p99_ms: 0,
      collision_ops: 0,
      collision_count: 0,
      collision_rate: 0,
    };
    for (let i = 0; i < runs; i++) {
      const result = await runOne({ ...this.config, concurrency, alpha });
      totals.tps += result.tps;
      totals.samples += result.samples;
      totals.committed_txns += result.committed_txns ?? 0;
      totals.p50_ms += result.p50_ms;
      totals.p95_ms += result.p95_ms;
      totals.p99_ms += result.p99_ms;
      totals.collision_ops += result.collision_ops;
      totals.collision_count += result.collision_count;
      totals.collision_rate += result.collision_rate;

      await new Promise((resolve) => setTimeout(resolve, 1000));
    }
    const avg = {
      tps: totals.tps / runs,
      samples: totals.samples / runs,
      committed_txns: totals.committed_txns / runs,
      p50_ms: totals.p50_ms / runs,
      p95_ms: totals.p95_ms / runs,
      p99_ms: totals.p99_ms / runs,
      collision_ops: totals.collision_ops / runs,
      collision_count: totals.collision_count / runs,
      collision_rate: totals.collision_rate / runs,
    };
    return avg;
  }

  async contentionTests(
    startAlpha: number = 1,
    endAlpha: number = 100,
    step: number = 1,
    concurrency: number = 50,
  ) {
    const results: { alpha: number; avgResult: RunResult }[] = [];
    for (let alpha = startAlpha; alpha <= endAlpha; alpha += step) {
      const avgResult = await this.runAvg(concurrency, alpha);
      results.push({ alpha, avgResult });

      await new Promise((resolve) => setTimeout(resolve, 1000));
    }
    return results;
  }

  async concurrencyTests(
    startConc: number = 1,
    endConc: number = 100,
    step: number = 1,
    alpha: number = 1,
  ) {
    const results: { concurrency: number; avgResult: RunResult }[] = [];
    for (let conc = startConc; conc <= endConc; conc += step) {
      const avgResult = await this.runAvg(conc, alpha);
      results.push({ concurrency: conc, avgResult });

      await new Promise((resolve) => setTimeout(resolve, 1000));
    }
    return results;
  }

  async concurrencyTestsMutiply(
    startConc: number = 1,
    endConc: number = 100,
    factor: number = 2,
    alpha: number = 1,
  ) {
    if (factor <= 1) {
      throw new Error('factor must be > 1 to avoid infinite loop');
    }

    const results: { concurrency: number; avgResult: RunResult }[] = [];

    for (let conc = startConc; conc <= endConc; conc *= factor) {
      const avgResult = await this.runAvg(conc, alpha);
      results.push({ concurrency: conc, avgResult });

      await new Promise((resolve) => setTimeout(resolve, 1000));
    }

    return results;
  }
}

const testDirUrl = new URL(`./tests/${testName}/`, import.meta.url);
const testDirPath = fileURLToPath(testDirUrl);

(async () => {
  const files = (await readdir(testDirPath)).filter(
    (f) => (f.endsWith('.ts') || f.endsWith('.js')) && !f.endsWith('.d.ts'),
  );

  const results: any[] = [];

  for (const file of files) {
    const mod = (await import(
      new URL(`./tests/${testName}/${file}`, import.meta.url).href
    )) as TestCaseModule;
    const tc = mod.default;

    if (connectors && !connectors.includes(tc.system)) continue;

    const makeConnector = (CONNECTORS as any)[tc.system];
    if (!makeConnector) throw new Error(`Unknown connector ${tc.system}`);

    const connector = makeConnector();

    let res: any;

    const config = { connector, scenario: tc.run, seconds, accounts };

    const tester = new BenchmarkTester(config);

    if (contentionTests) {
      res = await tester.contentionTests(
        contentionTests.startAlpha,
        contentionTests.endAlpha,
        contentionTests.step,
        contentionTests.concurrency,
      );
    } else if (concurrencyTests) {
      res = await tester.concurrencyTestsMutiply(
        concurrencyTests.startConc,
        concurrencyTests.endConc,
        concurrencyTests.step,
        concurrencyTests.alpha,
      );
    } else {
      res = await runOne({
        connector,
        scenario: tc.run,
        seconds,
        concurrency,
        accounts,
        alpha,
      });
    }

    results.push({
      system: connector.name,
      label: tc.label ?? file,
      file,
      seconds,
      concurrency,
      accounts,
      alpha,
      res,
    });
    console.log(`${file}:`, res);
  }

  const runData = {
    test: testName,
    seconds,
    concurrency,
    accounts,
    alpha,
    results,
  };
  const runsDir = fileURLToPath(new URL('../runs/', import.meta.url));
  await mkdir(runsDir, { recursive: true });
  const outFile = join(
    runsDir,
    `${testName}-${new Date().toISOString().replace(/[:.]/g, '-')}.json`,
  );
  await writeFile(outFile, JSON.stringify(runData, null, 2));

  console.log(`Wrote results to ${outFile}`);
})();
