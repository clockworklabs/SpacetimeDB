import { appendFileSync, existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { codexArguments, codexTranscriptDirectory, parseCodexResult, runCodexProcess }
  from '../src/agents/codex-protocol.js';
import { claudeRatesForModel } from '../src/evidence/claude-usage-cost.js';
import { runTranscriptAwareProcess } from '../src/agents/claude-terminal-recovery.js';
import type { PricingRates } from '../src/evidence/pricing-authority.js';
import { containerClaudeTranscriptReader } from './claude-transcript-reader.js';
import { CODING_CONTAINER_AGENT, CODING_CONTAINER_APP_ROOT } from '../src/runtime/coding-container-policy.js';
import { validateClaudeNativeSession, validateCodexNativeSession }
  from '../src/agents/native-session-validation.js';

type Invocation = { model: string; effort: string; baseUrl: string; resumeSession: string | null;
  maxBudgetUsd: string | null };
type ProcessOptions = Parameters<typeof runCodexProcess>[0] & {
  projects: string; containerId: string; marker: string; model: string;
  pricingRates: PricingRates | null; resumeSession: string | null;
};
interface CodingProvider {
  requiresBudget: boolean;
  executable: string;
  apiKeyEnvironment: string;
  credentialPath?: string;
  containerTranscripts: string;
  tokenEnvironment: string;
  environment(baseUrl: string): string[];
  projects(appDir: string): string;
  rates(model: string): PricingRates | null;
  args(options: Invocation): string[];
  run(options: ProcessOptions): ReturnType<typeof runCodexProcess>;
  result(stdout: string, appDir: string, invocationToken: string): Record<string, unknown> | null;
  validateContinuation(directory: string, sessionId: string, model: string): void;
}

const codexProvider: CodingProvider = {
  requiresBudget: true,
  executable: 'codex', apiKeyEnvironment: 'OPENAI_API_KEY',
  containerTranscripts: `${CODING_CONTAINER_AGENT.home}/.codex/sessions`,
  tokenEnvironment: 'MODEL_PROXY_TOKEN',
  environment: () => [`CODEX_HOME=${CODING_CONTAINER_AGENT.home}/.codex`],
  projects: appDir => join(codexTranscriptDirectory(appDir), 'sessions'),
  rates: () => null,
  args: codexArguments,
  run: runCodexProcess,
  validateContinuation: validateCodexNativeSession,
  result: (stdout, appDir, invocationToken) => {
    const result = parseCodexResult(stdout);
    const sessionId = result.session_id;
    const eventFile = typeof sessionId === 'string' && /^[0-9a-f-]{36}$/i.test(sessionId)
      ? `${sessionId}.events.jsonl` : `interrupted-${invocationToken}.events.jsonl`;
    const path = join(codexTranscriptDirectory(appDir), eventFile);
    const header = existsSync(path) ? '' : `${JSON.stringify({ type: 'stack_bench_context', cwd: '/app' })}\n`;
    appendFileSync(path, `${header}${stdout}\n`, { mode: 0o600 });
    return result;
  },
};

export const CODING_PROVIDERS = {
  anthropic: {
    requiresBudget: false,
    executable: 'claude', apiKeyEnvironment: 'ANTHROPIC_API_KEY',
    credentialPath: join(homedir(), '.claude', '.credentials.json'),
    containerTranscripts: `${CODING_CONTAINER_AGENT.home}/.claude/projects/-app`,
    tokenEnvironment: 'ANTHROPIC_AUTH_TOKEN',
    environment: baseUrl => [`ANTHROPIC_BASE_URL=${baseUrl}`, 'DISABLE_AUTOUPDATER=1', 'FORCE_PROMPT_CACHING_5M=1'],
    projects: appDir => join(homedir(), '.claude', 'projects',
      resolve(appDir).replace(/[\\/:]/g, '-').toLowerCase()),
    rates: claudeRatesForModel,
    validateContinuation: validateClaudeNativeSession,
    args: ({ model, effort, maxBudgetUsd, resumeSession }) => {
      return [
        '--print', '--output-format', 'json',
        // Isolate the session from project memory, plugins, and integrations.
        '--bare',
        '--permission-mode', 'acceptEdits',
        '--settings', JSON.stringify({ permissions: { allow: ['Bash'] } }),
        '--effort', effort,
        '--model', model,
        ...(maxBudgetUsd !== null ? ['--max-budget-usd', maxBudgetUsd] : []),
        // The app is the only directory a session may reach; inside the container
        // that is all there is, but the flag is kept so host and container runs are
        // configured identically.
        '--add-dir', CODING_CONTAINER_APP_ROOT,
        ...(resumeSession !== null ? ['--resume', resumeSession] : []),
      ];
    },
    run: options => {
      const transcriptReader = containerClaudeTranscriptReader(options.containerId, options.projects, options.env);
      return runTranscriptAwareProcess({ ...options, transcriptDirectory: options.projects,
        transcriptReader, transcriptSnapshot: transcriptReader.snapshot(), pollMs: 1_000 });
    },
    result: stdout => {
      try { return JSON.parse(stdout); }
      catch {
        for (const line of stdout.split(/\r?\n/).reverse()) {
          try { return JSON.parse(line); } catch { /* Keep looking. */ }
        }
      }
      return null;
    },
  },
  openai: codexProvider,
  openrouter: { ...codexProvider, apiKeyEnvironment: 'OPENROUTER_API_KEY' },
} satisfies Record<string, CodingProvider>;

export type CodingProviderId = keyof typeof CODING_PROVIDERS;

export function parseCodingProvider(value: string): CodingProviderId {
  if (!Object.hasOwn(CODING_PROVIDERS, value)) throw new Error(`unsupported coding provider: ${value}`);
  return value as CodingProviderId;
}
