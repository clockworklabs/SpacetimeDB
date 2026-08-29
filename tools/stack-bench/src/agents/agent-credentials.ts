import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

export interface AgentCredentialArgs {
  apiKey?: string;
  apiKeyFile?: string;
}

export interface CredentialAgentAdapter {
  readonly id: string;
  readonly apiKeyEnvironmentVariable?: string | null;
}

interface AgentCredentialOptions {
  readonly env?: NodeJS.ProcessEnv;
  readonly read?: (path: string, encoding: BufferEncoding) => string;
}

export function resolveAgentCredential<TArgs extends AgentCredentialArgs>(
  args: TArgs,
  adapter: CredentialAgentAdapter,
  { env = process.env, read = (path, encoding) => readFileSync(path, encoding) }:
    AgentCredentialOptions = {},
): TArgs & AgentCredentialArgs {
  const adapterFileVariable = adapter.apiKeyEnvironmentVariable
    ? `${adapter.apiKeyEnvironmentVariable}_FILE` : null;
  const configuredKeyFile = args.apiKeyFile
    ?? (env.STACK_BENCH_API_KEY_FILE ? resolve(env.STACK_BENCH_API_KEY_FILE) : null)
    ?? (adapterFileVariable && env[adapterFileVariable] ? resolve(env[adapterFileVariable]) : null);
  if ((args.apiKey || configuredKeyFile) && !adapter.apiKeyEnvironmentVariable) {
    throw new Error(`agent adapter ${adapter.id} does not accept an API key`);
  }
  if (args.apiKey && configuredKeyFile) {
    throw new Error('use only one of --api-key and --api-key-file');
  }
  if (configuredKeyFile) {
    const value = read(configuredKeyFile, 'utf8').trim();
    if (!value) throw new Error(`API key file is empty: ${configuredKeyFile}`);
    args.apiKey = value;
    args.apiKeyFile = configuredKeyFile;
  }
  return args;
}
