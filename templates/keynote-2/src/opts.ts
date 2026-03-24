import cac from 'cac';
import { normalizeStdbUrl } from './core/stdbUrl';
import {
  defaultBenchTestName,
  defaultDemoSystems,
  getSharedRuntimeDefaults,
  parseConnectorList,
  type BenchOptions,
  type ConcurrencyTests,
  type ContentionTests,
  type DemoOptions,
  type SharedRuntimeConfig,
  validConnectors,
} from './config.ts';

interface OptionConfigNone {
  type?: undefined;
}

interface OptionConfigString {
  type: 'string';
}

interface OptionConfigNumber {
  type: 'number';
}

interface OptionConfigBoolean {
  type: 'boolean';
}

interface OptionConfigStrings {
  type: 'strings';
  possibleValues: readonly string[];
}

type OptionConfig =
  | OptionConfigString
  | OptionConfigNumber
  | OptionConfigBoolean
  | OptionConfigStrings
  | OptionConfigNone;

class CLIParser {
  private readonly cli = cac();
  #configs: Record<string, OptionConfig> = {};

  constructor(usage = '[options]') {
    this.cli.globalCommand.ignoreOptionDefaultValue();
    this.cli.help().usage(usage);
  }

  option(
    rawName: string,
    description: string,
    config: OptionConfig = {},
  ): this {
    if (config.type === 'strings') {
      description += ` (valid values: ${config.possibleValues.join(', ')})`;
    }

    this.cli.option(rawName, description, {
      type: config.type === 'strings' ? [] : undefined,
    });

    const { name, isBoolean } =
      this.cli.globalCommand.options[this.cli.globalCommand.options.length - 1];
    this.#configs[name] = isBoolean && config.type === undefined
      ? { type: 'boolean' }
      : config;

    return this;
  }

  parse(
    argv: string[],
    { maxArgs = 0 }: { maxArgs?: number } = {},
  ) {
    const parsed = this.cli.parse(argv);

    this.cli.globalCommand.checkUnknownOptions();
    this.cli.globalCommand.checkOptionValue();
    this.cli.globalCommand.checkRequiredArgs();

    if (parsed.args.length > maxArgs) {
      throw new Error(
        `Unused args: ${parsed.args
          .slice(maxArgs)
          .map((arg) => `\`${arg}\``)
          .join(', ')}`,
      );
    }

    const { options } = parsed;

    if (options.help) {
      process.exit(0);
    }

    for (const [name, config] of Object.entries(this.#configs)) {
      if (options[name] === undefined) continue;

      switch (config.type) {
        case 'boolean':
          options[name] =
            typeof options[name] === 'boolean'
              ? options[name]
              : !(options[name] === '0' ||
                  options[name] === '' ||
                  options[name] === 'false');
          break;
        case 'number': {
          const value = Number(options[name]);
          if (!Number.isFinite(value)) {
            throw new Error(`invalid number '${options[name]}'`);
          }
          options[name] = value;
          break;
        }
        case 'strings':
          if (options[name]?.length === 1 && options[name][0] === undefined) {
            options[name] = undefined;
            break;
          }
          options[name] = parseConnectorList(
            options[name] as string | string[] | undefined,
            `--${name}`,
          );
          break;
      }
    }

    return parsed;
  }
}

const num = (): OptionConfig => ({ type: 'number' });
const str = (): OptionConfig => ({ type: 'string' });

function addSharedRuntimeOptions(parser: CLIParser): CLIParser {
  return parser
    .option('--seconds <seconds>', 'Number of seconds to benchmark for', num())
    .option('--concurrency <concurrency>', 'Concurrent clients to run', num())
    .option('--alpha <alpha>', 'Alpha value', num())
    .option('--accounts <num>', 'Number of accounts to run with', num())
    .option(
      '--initial-balance <balance>',
      'Initial balance for accounts',
      num(),
    )
    .option('--stdb-url <url>', 'SpacetimeDB url', str())
    .option('--stdb-module <name>', 'SpacetimeDB module name', str())
    .option('--stdb-module-path <dir>', 'SpacetimeDB module path', str())
    .option('--no-stdb-confirmed-reads', 'Disable confirmed reads')
    .option('--use-docker', 'Use docker')
    .option('--no-use-spacetime-metrics-endpoint', '')
    .option('--pool-max <num>', 'Max pool size for postgres', num())
    .option('--bun-url <url>', 'Bun server url', str())
    .option('--convex-url <url>', 'Convex server url', str())
    .option('--convex-dir <dir>', 'Convex directory', str())
    .option('--op-timeout-ms <num>', '', num())
    .option('--min-op-timeout-ms <num>', '', num())
    .option('--tail-slack-ms <num>', '', num())
    .option('--precomputed-transfer-pairs <num>', '', num())
    .option('--bench-pipelined', 'Force all systems to run pipelined', {
      type: 'boolean',
    })
    .option('--no-bench-pipelined', 'Disable request pipelining', {
      type: 'boolean',
    })
    .option(
      '--max-inflight-per-worker <num>',
      'When pipelining, max number of inflight requests allowed',
      num(),
    )
    .option('--log-errors', 'Log errors')
    .option('--verify-transactions', 'Verify transactions');
}

function resolveRuntimeOptions(
  options: Record<string, any>,
  defaults: SharedRuntimeConfig = getSharedRuntimeDefaults(),
): SharedRuntimeConfig {
  return {
    accounts: options.accounts ?? defaults.accounts,
    initialBalance: options.initialBalance ?? defaults.initialBalance,
    stdbUrl: normalizeStdbUrl(options.stdbUrl ?? defaults.stdbUrl),
    stdbModule: options.stdbModule ?? defaults.stdbModule,
    stdbModulePath: options.stdbModulePath ?? defaults.stdbModulePath,
    stdbConfirmedReads:
      options.stdbConfirmedReads ?? defaults.stdbConfirmedReads,
    useDocker: options.useDocker ?? defaults.useDocker,
    useSpacetimeMetricsEndpoint:
      options.useSpacetimeMetricsEndpoint ??
      defaults.useSpacetimeMetricsEndpoint,
    poolMax: options.poolMax ?? defaults.poolMax,
    bunUrl: options.bunUrl ?? defaults.bunUrl,
    convexUrl: options.convexUrl ?? defaults.convexUrl,
    convexDir: options.convexDir ?? defaults.convexDir,
    opTimeoutMs: options.opTimeoutMs ?? defaults.opTimeoutMs,
    minOpTimeoutMs: options.minOpTimeoutMs ?? defaults.minOpTimeoutMs,
    tailSlackMs: options.tailSlackMs ?? defaults.tailSlackMs,
    precomputedTransferPairs:
      options.precomputedTransferPairs ?? defaults.precomputedTransferPairs,
    benchPipelined: options.benchPipelined ?? defaults.benchPipelined,
    maxInflightPerWorker:
      options.maxInflightPerWorker ?? defaults.maxInflightPerWorker,
    logErrors: options.logErrors ?? defaults.logErrors,
    verifyTransactions:
      options.verifyTransactions ?? defaults.verifyTransactions,
  };
}

function parseNumericTuple(
  raw: string | string[] | undefined,
  label: string,
  expectedLength: number,
): number[] | undefined {
  if (raw === undefined) return undefined;

  const values = (Array.isArray(raw) ? raw : [raw])
    .flatMap((value) => value.split(','))
    .map((value) => value.trim())
    .filter(Boolean);

  if (values.length !== expectedLength) {
    throw new Error(
      `${label} expects ${expectedLength} values, got ${values.length}`,
    );
  }

  return values.map((value) => {
    const number = Number(value);
    if (!Number.isFinite(number)) {
      throw new Error(`invalid number '${value}'`);
    }
    return number;
  });
}

function collapseTupleOptionArgs(
  argv: string[],
  tupleOptionArities: Record<string, number>,
): string[] {
  const normalized = argv.slice(0, 2);

  for (let i = 2; i < argv.length; i++) {
    const token = argv[i]!;
    const arity = tupleOptionArities[token];

    if (arity === undefined) {
      normalized.push(token);
      continue;
    }

    const firstValue = argv[i + 1];
    if (!firstValue || firstValue.startsWith('--')) {
      throw new Error(`${token} expects ${arity} values`);
    }

    if (firstValue.includes(',')) {
      normalized.push(token, firstValue);
      i += 1;
      continue;
    }

    const values = argv.slice(i + 1, i + 1 + arity);
    if (values.length !== arity || values.some((value) => value.startsWith('--'))) {
      throw new Error(`${token} expects ${arity} values`);
    }

    normalized.push(token, values.join(','));
    i += arity;
  }

  return normalized;
}

export function parseDemoOptions(argv: string[] = process.argv): DemoOptions {
  const runtimeDefaults = getSharedRuntimeDefaults();
  const { options } = addSharedRuntimeOptions(new CLIParser('[options]'))
    .option('--systems <systems>', 'The systems to run against', {
      type: 'strings',
      possibleValues: validConnectors,
    })
    .option('--connectors <connectors>', 'Alias for --systems', {
      type: 'strings',
      possibleValues: validConnectors,
    })
    .option('--skip-prep', 'Skip prep')
    .option('--no-animation', 'No animation')
    .parse(argv);

  const runtimeOptions = resolveRuntimeOptions(options, runtimeDefaults);

  return {
    ...runtimeOptions,
    seconds: options.seconds ?? 10,
    concurrency: options.concurrency ?? 10,
    alpha: options.alpha ?? 1.5,
    systems:
      options.systems ?? options.connectors ?? [...defaultDemoSystems],
    skipPrep: options.skipPrep ?? false,
    noAnimation:
      options.animation === undefined ? false : !options.animation,
  };
}

export function parseBenchOptions(argv: string[] = process.argv): BenchOptions {
  const runtimeDefaults = getSharedRuntimeDefaults();
  const normalizedArgv = collapseTupleOptionArgs(argv, {
    '--contention-tests': 4,
    '--concurrency-tests': 4,
  });
  const { args, options } = addSharedRuntimeOptions(
    new CLIParser('[test-name] [options]'),
  )
    .option('--connectors <connectors>', 'The connectors to run against', {
      type: 'strings',
      possibleValues: validConnectors,
    })
    .option('--systems <systems>', 'Alias for --connectors', {
      type: 'strings',
      possibleValues: validConnectors,
    })
    .option(
      '--contention-tests <spec>',
      'Run alpha sweep as start,end,step,concurrency',
      str(),
    )
    .option(
      '--concurrency-tests <spec>',
      'Run concurrency sweep as start,end,factor,alpha',
      str(),
    )
    .parse(normalizedArgv, { maxArgs: 1 });

  const runtimeOptions = resolveRuntimeOptions(options, runtimeDefaults);

  const contentionValues = parseNumericTuple(
    options.contentionTests as string | string[] | undefined,
    '--contention-tests',
    4,
  );
  const concurrencyValues = parseNumericTuple(
    options.concurrencyTests as string | string[] | undefined,
    '--concurrency-tests',
    4,
  );

  const contentionTests: ContentionTests | null = contentionValues
    ? {
        startAlpha: contentionValues[0]!,
        endAlpha: contentionValues[1]!,
        step: contentionValues[2]!,
        concurrency: contentionValues[3]!,
      }
    : null;
  const concurrencyTests: ConcurrencyTests | null = concurrencyValues
    ? {
        startConc: concurrencyValues[0]!,
        endConc: concurrencyValues[1]!,
        step: concurrencyValues[2]!,
        alpha: concurrencyValues[3]!,
      }
    : null;

  return {
    ...runtimeOptions,
    testName: args[0] ?? defaultBenchTestName,
    seconds: options.seconds ?? 1,
    concurrency:
      contentionTests?.concurrency ?? options.concurrency ?? 10,
    alpha: concurrencyTests?.alpha ?? options.alpha ?? 1.5,
    connectors: options.connectors ?? options.systems ?? null,
    contentionTests,
    concurrencyTests,
  };
}
