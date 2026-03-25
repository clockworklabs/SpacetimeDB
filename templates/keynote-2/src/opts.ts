import cac from 'cac';
import { normalizeStdbUrl } from './core/stdbUrl';

export const validConnectors = [
  'convex',
  'spacetimedb',
  'spacetimedbRustClient',
  'bun',
  'postgres_rpc',
  'cockroach_rpc',
  'sqlite_rpc',
  'supabase_rpc',
  'planetscale_pg_rpc',
] as const;
export type ConnectorKey = (typeof validConnectors)[number];

interface OptionConfigBase {
  env?: string;
}
interface OptionConfigNone extends OptionConfigBase {
  type?: undefined;
}
interface OptionConfigString extends OptionConfigBase {
  type: 'string';
  default?: string;
}
interface OptionConfigNumber extends OptionConfigBase {
  type: 'number';
  default?: number;
}
interface OptionConfigBoolean extends OptionConfigBase {
  type: 'boolean';
  default?: boolean;
}
interface OptionConfigStrings extends OptionConfigBase {
  type: 'strings';
  possibleValues: readonly string[];
  default?: readonly string[];
}
type OptionConfig =
  | OptionConfigString
  | OptionConfigNumber
  | OptionConfigBoolean
  | OptionConfigStrings
  | OptionConfigNone;

class CLIParser {
  constructor() {
    this.cac.globalCommand.ignoreOptionDefaultValue();
    this.cac.help().usage('[options]');
  }
  cac = cac();
  #configs: Record<string, OptionConfig> = {};
  option(
    rawName: string,
    description: string,
    config: OptionConfig = {},
  ): this {
    if (config.type === 'strings') {
      description += ` (valid values: ${config.possibleValues.join(', ')})`;
    }
    if (config.env) {
      description += ` [env: ${config.env}]`;
    }
    this.cac.option(rawName, description, {
      default: 'default' in config ? config.default : undefined,
      type: config.type === 'strings' ? [] : undefined,
    });
    const { name, isBoolean, negated } =
      this.cac.globalCommand.options[this.cac.globalCommand.options.length - 1];
    this.#configs[name] =
      isBoolean && config.type === undefined
        ? { type: 'boolean', env: config.env, default: negated }
        : config;
    return this;
  }

  parse() {
    const args = this.cac.parse();

    this.cac.globalCommand.checkUnknownOptions();
    this.cac.globalCommand.checkOptionValue();
    this.cac.globalCommand.checkRequiredArgs();
    this.cac.globalCommand.checkUnusedArgs();

    const { options } = args;

    if (options.help) {
      process.exit(0);
    }

    for (const [name, config] of Object.entries(this.#configs)) {
      if (config.env) options[name] ??= process.env[config.env];

      let parser: (s: any) => any = (s) => s;
      switch (config.type) {
        case 'boolean':
          parser = (s) =>
            typeof s === 'boolean'
              ? s
              : !(s === '0' || s === '' || s === 'false');
          break;
        case 'number':
          parser = (s) => {
            const n = Number(s);
            if (Number.isFinite(n)) return n;
            throw new Error(`invalid number '${s}'`);
          };
          break;
        case 'strings':
          if (options[name]?.length === 1 && options[name][0] === undefined) {
            options[name] = undefined;
          }
          parser = (s: string | string[]) =>
            (Array.isArray(s) ? s : s.split(',')).flat().map((s) => {
              const x = s.trim();
              if (!config.possibleValues.includes(x)) {
                throw new Error(`${x} is not a valid value for this option`);
              }
              return x;
            });
          break;
      }

      if (options[name] !== undefined) {
        options[name] = parser(options[name]);
      } else if ('default' in config) {
        options[name] = config.default;
      }
    }

    return args;
  }
}

const num = (defaultVal: number, env?: string): OptionConfig => ({
  type: 'number',
  default: defaultVal,
  env,
});
const str = (defaultVal: string, env?: string): OptionConfig => ({
  type: 'string',
  default: defaultVal,
  env,
});

const args = new CLIParser()
  .option('--seconds <seconds>', 'Number of seconds to benchmark for', num(10))
  .option('--concurrency <concurrency>', 'Concurrent clients to run', num(10))
  .option('--alpha <alpha>', 'Alpha value', num(1.5))
  .option('--systems <systems>', 'The systems to run against', {
    type: 'strings',
    possibleValues: validConnectors,
    default: ['convex', 'spacetimedb'],
  })
  .option('--skip-prep', 'Skip prep')
  .option('--no-animation', 'No animation')
  .option(
    '--accounts <num>',
    'Number of accounts to run with',
    num(100_000, 'SEED_ACCOUNTS'),
  )
  .option(
    '--initial-balance <balance>',
    'Initial balance for accounts',
    num(10_000_000, 'SEED_INITIAL_BALANCE'),
  )
  .option(
    '--stdb-url <url>',
    'SpacetimeDB url',
    str('127.0.0.1:3000', 'STDB_URL'),
  )
  .option(
    '--stdb-module <name>',
    'SpacetimeDB module name',
    str('test-1', 'STDB_MODULE'),
  )
  .option(
    '--stdb-module-path <dir>',
    'SpacetimeDB module path',
    str('./spacetimedb', 'STDB_MODULE_PATH'),
  )
  .option('--no-stdb-confirmed-reads', 'Disable confirmed reads', {
    env: 'STDB_CONFIRMED_READS',
  })
  .option('--use-docker', 'Use docker', { env: 'USE_DOCKER' })
  .option('--no-use-spacetime-metrics-endpoint', '', {
    env: 'SPACETIME_METRICS_ENDPOINT',
  })
  .option(
    '--pool-max <num>',
    'Max pool size for postgres',
    num(1000, 'MAX_POOL'),
  )
  .option(
    '--bun-url <url>',
    'Bun server url',
    str('http://127.0.0.1:4000', 'BUN_URL'),
  )
  .option(
    '--convex-url <url>',
    'Convex server url',
    str('http://127.0.0.1:3210', 'CONVEX_URL'),
  )
  .option(
    '--convex-dir <dir>',
    'Convex directory',
    str('./convex-app', 'CONVEX_DIR'),
  )
  .option('--op-timeout-ms <num>', '', num(15000, 'BENCH_OP_TIMEOUT_MS'))
  .option('--min-op-timeout-ms <num>', '', num(250, 'MIN_OP_TIMEOUT_MS'))
  .option('--tail-slack-ms <num>', '', num(1000, 'TAIL_SLACK_MS'))
  .option(
    '--precomputed-transfer-pairs <num>',
    '',
    num(10_000_000, 'BENCH_PRECOMPUTED_TRANSFER_PAIRS'),
  )
  .option('--bench-pipelined', 'Force all systems to run pipelined', {
    type: 'boolean',
    env: 'BENCH_PIPELINED',
  })
  .option('--no-bench-pipelined', 'Disable request pipelining', {
    type: 'boolean',
    env: 'BENCH_PIPELINED',
  })
  .option(
    '--max-inflight-per-worker <num>',
    'When pipelining, max number of inflight requests allowed',
    { type: 'number', env: 'MAX_INFLIGHT_PER_WORKER' },
  )
  .option('--log-errors', 'Log errors', { env: 'LOG_ERRORS' })
  .option('--verify-transactions', 'Verify transactions', { env: 'VERIFY' })
  .parse();

const opts = args.options;

export const seconds: number = opts.seconds;
export const concurrency: number = opts.concurrency;
export const alpha: number = opts.alpha;
export const systems: ConnectorKey[] = opts.systems;
export const skipPrep: boolean = opts.skipPrep;
export const noAnimation: boolean = !opts.animation;

export const accounts: number = opts.accounts;
export const initialBalance: number = opts.initialBalance;

export const stdbUrl: string = normalizeStdbUrl(opts.stdbUrl);
export const stdbModule: string = opts.stdbModule;
export const stdbModulePath: string = opts.stdbModulePath;
export const stdbConfirmedReads: boolean = opts.stdbConfirmedReads;

export const useDocker: boolean = opts.useDocker;
export const useSpacetimeMetricsEndpoint: boolean =
  opts.useSpacetimeMetricsEndpoint;

export const poolMax: number = opts.poolMax;
export const bunUrl: string = opts.bunUrl;
export const convexUrl: string = opts.convexUrl;
export const convexDir: string = opts.convexDir;

export const opTimeoutMs: number = opts.opTimeoutMs;
export const minOpTimeoutMs: number = opts.minOpTimeoutMs;
export const tailSlackMs: number = opts.tailSlackMs;
export const precomputedTransferPairs: number = opts.precomputedTransferPairs;
export const benchPipelined: boolean | undefined = opts.benchPipelined;
export const maxInflightPerWorker: number | undefined =
  opts.maxInflightPerWorker;
export const logErrors: boolean = opts.logErrors;
export const verifyTransactions: boolean = opts.verifyTransactions;
