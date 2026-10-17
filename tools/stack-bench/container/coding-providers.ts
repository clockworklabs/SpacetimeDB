import { appendFileSync, existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { codexArguments, codexTranscriptDirectory, parseCodexResult, runCodexProcess }
  from '../src/agents/codex-protocol.js';
import type { CodexUsage } from '../src/agents/codex-protocol.js';
import { claudeRatesForModel } from '../src/evidence/claude-usage-cost.js';
import { runTranscriptAwareProcess } from '../src/agents/claude-terminal-recovery.js';
import type { PricingRates } from '../src/evidence/pricing-authority.js';
import { containerClaudeTranscriptReader } from './claude-transcript-reader.js';
import { CODING_CONTAINER_AGENT, CODING_CONTAINER_APP_ROOT } from '../src/runtime/coding-container-policy.js';
import { validateAntigravityNativeSession, validateClaudeNativeSession, validateCodexNativeSession, validateGrokNativeSession }
  from '../src/agents/native-session-validation.js';
import { grokArguments, grokTranscriptDirectory, parseGrokResult } from '../src/agents/grok-protocol.js';
import { ANTIGRAVITY_LAUNCHER, antigravityArguments, antigravityFinishedAnswer, antigravityTranscriptDirectory,
  parseAntigravityResult }
  from '../src/agents/antigravity-protocol.js';

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
  environment(baseUrl: string, mode: 'api-key' | 'subscription-token'): string[];
  projects(appDir: string): string;
  rates(model: string): PricingRates | null;
  args(options: Invocation): string[];
  run(options: ProcessOptions): ReturnType<typeof runCodexProcess>;
  result(stdout: string, appDir: string, invocationToken: string, prior: CodexUsage | null): Record<string, unknown> | null;
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
  result: (stdout, appDir, invocationToken, prior) => {
    const result = parseCodexResult(stdout, prior);
    const sessionId = result.session_id;
    const eventFile = typeof sessionId === 'string' && /^[0-9a-f-]{36}$/i.test(sessionId)
      ? `${sessionId}.events.jsonl` : `interrupted-${invocationToken}.events.jsonl`;
    const path = join(codexTranscriptDirectory(appDir), eventFile);
    const header = existsSync(path) ? '' : `${JSON.stringify({ type: 'stack_bench_context', cwd: '/app' })}\n`;
    appendFileSync(path, `${header}${stdout}\n`, { mode: 0o600 });
    return result;
  },
};

// Grok signs in through its external auth command with the broker's session token,
// which the broker exchanges for the real SuperGrok token. The CLI never sees it.
const GROK_SIGN_IN = `printf '{"access_token":"%s","expires_in":86400,"issuer":"https://auth.x.ai"}' "$GROK_BROKER_TOKEN"`;
const grokProvider: CodingProvider = {
  requiresBudget: true,
  executable: 'sh', apiKeyEnvironment: 'XAI_API_KEY',
  containerTranscripts: `${CODING_CONTAINER_AGENT.home}/.grok/sessions`,
  tokenEnvironment: 'GROK_BROKER_TOKEN',
  // Telemetry and session uploads would reach the broker as refused requests. The
  // turn summary is a display call over the whole turn that the CLI exits without
  // awaiting, so a long session leaves it unpriced.
  environment: baseUrl => [`GROK_CLI_CHAT_PROXY_BASE_URL=${baseUrl}/v1`, `GROK_AUTH_PROVIDER_COMMAND=${GROK_SIGN_IN}`,
    'GROK_DISABLE_AUTOUPDATER=1', 'GROK_TELEMETRY_ENABLED=0', 'GROK_TELEMETRY_TRACE_UPLOAD=0', 'DISABLE_TELEMETRY=1',
    'GROK_TURN_SUMMARY=0'],
  projects: appDir => join(grokTranscriptDirectory(appDir), 'sessions'),
  rates: () => null,
  // Sign-in prints the account's email address; none of its output may reach evidence.
  args: options => ['-c', 'grok login < /dev/null > /dev/null 2>&1 || { echo "Grok sign-in failed" >&2; exit 1; }; exec grok "$@"',
    'grok', ...grokArguments(options)],
  run: runCodexProcess,
  validateContinuation: validateGrokNativeSession,
  // The controller keeps the stream as the session's audited transcript, beside the
  // agent-writable native sessions.
  result: (stdout, appDir, invocationToken) => {
    const result = parseGrokResult(stdout);
    const name = typeof result.session_id === 'string' ? result.session_id : `interrupted-${invocationToken}`;
    appendFileSync(join(grokTranscriptDirectory(appDir), `${name}.events.jsonl`), `${stdout}\n`, { mode: 0o600 });
    return result;
  },
};

// The CLI sends the broker's session token as its Gemini key or account token, and the
// broker sends the real one to Google. Its whole state directory persists between sessions
// so a session can resume.
const googleProvider: CodingProvider = {
  requiresBudget: true,
  executable: 'sh', apiKeyEnvironment: 'GEMINI_API_KEY',
  containerTranscripts: `${CODING_CONTAINER_AGENT.home}/.gemini/antigravity-cli`,
  tokenEnvironment: 'GEMINI_API_KEY',
  environment: (baseUrl, mode) => mode === 'subscription-token' ? [`CLOUD_CODE_URL=${baseUrl}`, `HTTPS_PROXY=${baseUrl}`]
    : [`GOOGLE_GEMINI_BASE_URL=${baseUrl}`],
  projects: appDir => join(antigravityTranscriptDirectory(appDir), 'state'),
  rates: () => null,
  args: options => ['-c', ANTIGRAVITY_LAUNCHER, 'agy', ...antigravityArguments(options)],
  run: options => {
    const startedAt = Date.now();
    return runCodexProcess({ ...options,
      finishedAnswer: () => antigravityFinishedAnswer(options.projects, options.marker, startedAt) });
  },
  validateContinuation: validateAntigravityNativeSession,
  // The controller keeps the stream as the session's audited transcript, beside the
  // agent-writable native state.
  result: (stdout, appDir, invocationToken, prior) => {
    const result = parseAntigravityResult(stdout, prior);
    const name = typeof result.session_id === 'string' ? result.session_id : `interrupted-${invocationToken}`;
    appendFileSync(join(antigravityTranscriptDirectory(appDir), `${name}.events.jsonl`), `${stdout}\n`, { mode: 0o600 });
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
  xai: grokProvider,
  google: googleProvider,
} satisfies Record<string, CodingProvider>;

export type CodingProviderId = keyof typeof CODING_PROVIDERS;

export function parseCodingProvider(value: string): CodingProviderId {
  if (!Object.hasOwn(CODING_PROVIDERS, value)) throw new Error(`unsupported coding provider: ${value}`);
  return value as CodingProviderId;
}
