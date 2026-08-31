import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { closeSync, existsSync, openSync, readFileSync, readSync,
  readdirSync, statSync } from 'node:fs';
import { basename, join, sep } from 'node:path';

import { claudeRatesForModel, normalizeClaudeUsage,
  priceClaudeUsage } from '../evidence/claude-usage-cost.js';
import { validatePricingRates } from '../evidence/pricing-authority.js';
import type { PricingRates } from '../evidence/pricing-authority.js';

const UUID_FILE = /^([0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})\.jsonl$/i;

export type ClaudeTranscriptSnapshot = Map<string, number>;

type JsonObject = Record<string, unknown>;

interface ClaudeCliResult extends JsonObject {
  is_error: boolean;
  session_id: string;
  result: string;
  total_cost_usd: number;
  num_turns: number;
  usage: JsonObject;
}

interface TranscriptMessage extends JsonObject {
  id?: unknown;
  model?: unknown;
  stop_reason?: unknown;
  content?: unknown;
  usage?: unknown;
}

interface TranscriptRecord extends JsonObject {
  type?: unknown;
  isSidechain?: unknown;
  sessionId?: unknown;
  requestId?: unknown;
  uuid?: unknown;
  message?: unknown;
}

interface AssistantUsageRecord extends TranscriptRecord {
  message: TranscriptMessage;
}

export interface ClaudeTerminalRecoveryEvidence {
  schemaVersion: 1;
  kind: 'terminal-transcript';
  marker: string;
  transcript: string;
  costSource: 'transcript-usage';
  pricedModels: unknown[];
}

export interface RecoveredClaudeTerminalResult {
  is_error: false;
  session_id: string;
  result: string;
  total_cost_usd: number;
  num_turns: number;
  usage: {
    input_tokens: number;
    output_tokens: number;
    cache_creation_input_tokens: number;
    cache_read_input_tokens: number;
  };
  terminal_recovery: ClaudeTerminalRecoveryEvidence;
}

interface RecoverClaudeTerminalOptions {
  directory: string;
  snapshot: ClaudeTranscriptSnapshot;
  marker: string;
  model: string;
  pricingRates?: PricingRates | null;
  resumeSession?: string | null;
}

export interface TerminalProcessRecoveryEvidence {
  schemaVersion: 1;
  kind: 'terminal-process';
  marker: string;
  transcript: string;
  resultSource: 'cli-json';
}

export interface TranscriptAwareProcessResult {
  status: number | null;
  signal: NodeJS.Signals | null;
  stdout: string;
  stderr: string;
  error: unknown;
  terminalRecovery?: ClaudeTerminalRecoveryEvidence | TerminalProcessRecoveryEvidence;
}

interface RunTranscriptAwareProcessOptions {
  command: string;
  args: string[];
  input?: string | null;
  env?: NodeJS.ProcessEnv;
  timeoutMs: number;
  transcriptDirectory: string;
  transcriptSnapshot: ClaudeTranscriptSnapshot;
  marker: string;
  model: string;
  resumeSession?: string | null;
  pricingRates?: PricingRates | null;
  exitGraceMs?: number;
  pollMs?: number;
  maxBuffer?: number;
  terminate?: (
    child: ChildProcessWithoutNullStreams,
    reason: string,
  ) => unknown | PromiseLike<unknown>;
}

function isJsonObject(value: unknown): value is JsonObject {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function transcriptMessage(value: unknown): TranscriptMessage | null {
  return isJsonObject(value) ? value : null;
}

function transcriptContent(message: TranscriptMessage): JsonObject[] {
  return Array.isArray(message.content) ? message.content.filter(isJsonObject) : [];
}

function isAssistantUsageRecord(record: TranscriptRecord): record is AssistantUsageRecord {
  const message = transcriptMessage(record.message);
  return record.type === 'assistant' && message !== null && Boolean(message.usage);
}

export function snapshotClaudeTranscripts(directory: string): ClaudeTranscriptSnapshot {
  const snapshot: ClaudeTranscriptSnapshot = new Map();
  if (!existsSync(directory)) return snapshot;
  (function walk(path: string): void {
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      const child = join(path, entry.name);
      if (entry.isDirectory()) walk(child);
      else if (entry.name.endsWith('.jsonl')) snapshot.set(child, statSync(child).size);
    }
  })(directory);
  return snapshot;
}

function markerPresent(text: string, marker: string): boolean {
  return new RegExp(`(?:^|\\s)${marker}(?=\\s|$)`).test(text);
}

function addedRecords(path: string, initialSize: number): TranscriptRecord[] {
  const value = readFileSync(path, 'utf8');
  // Transcript records are ASCII JSON around UTF-8 string content. Slicing the
  // byte buffer, not the JavaScript string, keeps the saved offset exact.
  const tail = Buffer.from(value).subarray(initialSize).toString('utf8');
  return tail.split(/\r?\n/).filter(Boolean).flatMap((line): TranscriptRecord[] => {
    try {
      const parsed: unknown = JSON.parse(line);
      return isJsonObject(parsed) ? [parsed] : [];
    } catch { return []; }
  });
}

export function parseCompleteClaudeCliResult(raw: unknown): ClaudeCliResult | null {
  const value = String(raw ?? '').trim();
  if (!value) return null;
  let parsed: unknown = null;
  try { parsed = JSON.parse(value); } catch {
    for (const line of value.split(/\r?\n/).reverse()) {
      try { parsed = JSON.parse(line); break; } catch { /* keep looking */ }
    }
  }
  if (!isJsonObject(parsed)
    || typeof parsed.is_error !== 'boolean'
    || typeof parsed.session_id !== 'string' || !parsed.session_id
    || typeof parsed.result !== 'string'
    || typeof parsed.total_cost_usd !== 'number'
    || !Number.isFinite(parsed.total_cost_usd) || parsed.total_cost_usd < 0
    || typeof parsed.num_turns !== 'number'
    || !Number.isSafeInteger(parsed.num_turns) || parsed.num_turns < 0
    || !isJsonObject(parsed.usage)) {
    return null;
  }
  try { normalizeClaudeUsage(parsed.usage); } catch { return null; }
  return parsed as ClaudeCliResult;
}

function candidateTranscriptPaths(
  directory: string,
  snapshot: ClaudeTranscriptSnapshot,
  resumeSession: string | null | undefined,
): string[] {
  const normalizedResume = resumeSession?.toLowerCase() ?? null;
  if (normalizedResume) return [join(directory, `${normalizedResume}.jsonl`)];
  return existsSync(directory)
    ? readdirSync(directory).filter(name => UUID_FILE.test(name)).map(name => join(directory, name))
      .filter(path => !snapshot.has(path))
    : [];
}

function hasCompletionMarker(
  directory: string,
  snapshot: ClaudeTranscriptSnapshot,
  marker: string,
  resumeSession: string | null | undefined,
): boolean {
  for (const path of candidateTranscriptPaths(directory, snapshot, resumeSession)) {
    if (!existsSync(path)) continue;
    const size = statSync(path).size;
    const initialSize = snapshot.get(path) ?? 0;
    if (size <= initialSize) continue;
    // The marker is at the end of the terminal text. A small tail probe avoids
    // reparsing a growing multi-megabyte transcript four times per second.
    const start = Math.max(initialSize, size - 128 * 1024);
    const buffer = Buffer.allocUnsafe(size - start);
    const handle = openSync(path, 'r');
    try { readSync(handle, buffer, 0, buffer.length, start); }
    finally { closeSync(handle); }
    if (buffer.includes(Buffer.from(marker))
      && buffer.includes(Buffer.from('"stop_reason":"end_turn"'))) return true;
  }
  return false;
}

export function recoverClaudeTerminalResult({ directory, snapshot, marker, model,
  pricingRates = null, resumeSession = null }:
  RecoverClaudeTerminalOptions): RecoveredClaudeTerminalResult | null {
  const requestedRates = pricingRates === null ? null
    : validatePricingRates(pricingRates, { at: 'terminal recovery pricing rates' });
  const normalizedResume = resumeSession?.toLowerCase() ?? null;
  const candidates = candidateTranscriptPaths(directory, snapshot, resumeSession);
  const matches: RecoveredClaudeTerminalResult[] = [];
  for (const path of candidates) {
    if (!existsSync(path)) continue;
    const initialSize = snapshot.get(path) ?? 0;
    if (statSync(path).size <= initialSize) continue;
    const sessionId = basename(path).match(UUID_FILE)?.[1];
    if (!sessionId || (normalizedResume && sessionId.toLowerCase() !== normalizedResume)) continue;
    const mainRecords = addedRecords(path, initialSize)
      .filter((record): record is AssistantUsageRecord => isAssistantUsageRecord(record)
        && record.isSidechain !== true && record.sessionId === sessionId);
    const terminal = mainRecords.findLast(record => record.message.stop_reason === 'end_turn'
      && transcriptContent(record.message).some(content => content.type === 'text'
        && markerPresent(String(content.text ?? ''), marker)));
    if (!terminal) continue;
    const records = [...mainRecords];
    const nestedRoot = join(directory, sessionId);
    if (existsSync(nestedRoot)) {
      for (const nestedPath of snapshotClaudeTranscripts(nestedRoot).keys()) {
        if (!nestedPath.startsWith(`${nestedRoot}${sep}`)) continue;
        records.push(...addedRecords(nestedPath, snapshot.get(nestedPath) ?? 0)
          .filter(isAssistantUsageRecord));
      }
    }
    const billed = new Map<unknown, AssistantUsageRecord>();
    for (const record of records) {
      const requestId = record.requestId ?? record.message.id ?? record.uuid;
      if (requestId && !billed.has(requestId)) billed.set(requestId, record);
    }
    if (!billed.size) throw new Error('terminal transcript has no billable assistant usage');
    const usage = { input_tokens: 0, output_tokens: 0,
      cache_creation_input_tokens: 0, cache_read_input_tokens: 0 };
    let totalCostUsd = 0;
    const pricedModels = new Set<unknown>();
    for (const record of billed.values()) {
      const actualModel = record.message.model ?? model;
      const rates = requestedRates ?? claudeRatesForModel(actualModel);
      if (!rates) {
        throw Object.assign(
          new Error(`terminal recovery has no recorded pricing for model ${actualModel}`),
          { code: 'CLAUDE_TERMINAL_RECOVERY_UNAVAILABLE' });
      }
      pricedModels.add(actualModel);
      const item = record.message.usage;
      totalCostUsd += priceClaudeUsage(item, rates);
      const tokens = normalizeClaudeUsage(item);
      usage.input_tokens += tokens.input;
      usage.output_tokens += tokens.output;
      usage.cache_creation_input_tokens += tokens.cacheWrite5m + tokens.cacheWrite1h;
      usage.cache_read_input_tokens += tokens.cacheRead;
    }
    const result = transcriptContent(terminal.message)
      .filter(content => content.type === 'text').map(content => content.text).join('\n');
    matches.push({ is_error: false, session_id: sessionId, result,
      total_cost_usd: totalCostUsd, num_turns: billed.size, usage,
      terminal_recovery: { schemaVersion: 1, kind: 'terminal-transcript', marker,
        transcript: basename(path), costSource: 'transcript-usage',
        pricedModels: [...pricedModels].sort() } });
  }
  if (matches.length > 1) {
    throw new Error('more than one active Claude transcript reached the completion marker');
  }
  return matches[0] ?? null;
}

export function runTranscriptAwareProcess({ command, args, input, env, timeoutMs,
  transcriptDirectory, transcriptSnapshot, marker, model, resumeSession = null,
  pricingRates = null,
  exitGraceMs = 15_000, pollMs = 250, maxBuffer = 256 * 1024 * 1024,
  terminate }: RunTranscriptAwareProcessOptions): Promise<TranscriptAwareProcessResult> {
  return new Promise<TranscriptAwareProcessResult>(resolve => {
    const child = spawn(command, args, { env, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    let stdoutBytes = 0;
    let stderrBytes = 0;
    let error: unknown = null;
    let recovered: RecoveredClaudeTerminalResult | null = null;
    let recoveryError: unknown = null;
    let terminalSeenAt: number | null = null;
    let stopping = false;
    const stop = (reason: string): void => {
      if (stopping) return;
      stopping = true;
      Promise.resolve(terminate?.(child, reason)).catch(() => child.kill('SIGKILL'));
    };
    const append = (target: Buffer[], chunk: Buffer, stdoutStream: boolean): void => {
      target.push(chunk);
      if (stdoutStream) stdoutBytes += chunk.length; else stderrBytes += chunk.length;
      if (stdoutBytes + stderrBytes > maxBuffer && !error) {
        error = Object.assign(new Error('coding session output exceeded maxBuffer'), { code: 'ENOBUFS' });
        stop('max-buffer');
      }
    };
    child.stdout.on('data', chunk => append(stdout, chunk, true));
    child.stderr.on('data', chunk => append(stderr, chunk, false));
    child.once('error', value => { error = value; });
    child.stdin.end(input ?? '');
    const poll = setInterval(() => {
      if (recovered) return;
      if (recoveryError) {
        if (terminalSeenAt !== null && Date.now() - terminalSeenAt >= exitGraceMs) {
          stop('terminal-recovery-unavailable');
        }
        return;
      }
      try {
        if (!hasCompletionMarker(transcriptDirectory, transcriptSnapshot, marker, resumeSession)) {
          return;
        }
        const found = recoverClaudeTerminalResult({ directory: transcriptDirectory,
          snapshot: transcriptSnapshot, marker, model, pricingRates, resumeSession });
        if (!found) return;
        if (terminalSeenAt === null) terminalSeenAt = Date.now();
        if (Date.now() - terminalSeenAt >= exitGraceMs) {
          recovered = found;
          stop('terminal-transcript');
        }
      } catch (value) {
        recoveryError = value;
        if (isJsonObject(value) && value.code === 'CLAUDE_TERMINAL_RECOVERY_UNAVAILABLE') {
          terminalSeenAt = Date.now();
        }
      }
    }, pollMs);
    const timeout = setTimeout(() => {
      if (!error) error = Object.assign(new Error('coding session timed out'), { code: 'ETIMEDOUT' });
      stop('timeout');
    }, timeoutMs);
    child.once('close', (status, signal) => {
      clearInterval(poll);
      clearTimeout(timeout);
      const stdoutText = Buffer.concat(stdout).toString('utf8');
      const stderrText = Buffer.concat(stderr).toString('utf8');
      if (status === 0 && !error && parseCompleteClaudeCliResult(stdoutText)) {
        resolve({ status, signal, stdout: stdoutText, stderr: stderrText, error: null });
        return;
      }
      if (recovered) {
        const cliResult = parseCompleteClaudeCliResult(stdoutText);
        if (cliResult) {
          const terminalRecovery: TerminalProcessRecoveryEvidence = {
            schemaVersion: 1, kind: 'terminal-process', marker,
            transcript: recovered.terminal_recovery.transcript, resultSource: 'cli-json' };
          resolve({ status: 0, signal,
            stdout: `${JSON.stringify({ ...cliResult, terminal_recovery: terminalRecovery })}\n`,
            stderr: stderrText, error: null, terminalRecovery });
          return;
        }
        resolve({ status: 0, signal, stdout: `${JSON.stringify(recovered)}\n`, stderr: stderrText,
          error: null, terminalRecovery: recovered.terminal_recovery });
        return;
      }
      if (recoveryError && !error && status !== 0) error = recoveryError;
      resolve({ status, signal, stdout: stdoutText, stderr: stderrText, error });
    });
  });
}
