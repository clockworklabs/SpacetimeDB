import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { basename, join, sep } from 'node:path';

import { claudeRatesForModel, normalizeClaudeUsage,
  priceClaudeUsage } from '../evidence/claude-usage-cost.js';
import { validatePricingRates } from '../evidence/pricing-authority.js';
import type { PricingRates } from '../evidence/pricing-authority.js';

const UUID_FILE = /^([0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})\.jsonl$/i;

export type ClaudeTranscriptSnapshot = Map<string, number>;

export interface ClaudeTranscriptReader {
  snapshot(): ClaudeTranscriptSnapshot;
  read(path: string, start: number, length: number): Buffer;
}

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
  pricedModels: string[];
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
  reader: ClaudeTranscriptReader;
  currentSnapshot?: ClaudeTranscriptSnapshot;
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
  transcriptReader: ClaudeTranscriptReader;
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

function markerPresent(text: string, marker: string): boolean {
  return new RegExp(`(?:^|\\s)${marker}(?=\\s|$)`).test(text);
}

function addedRecords(reader: ClaudeTranscriptReader, path: string, initialSize: number,
  size: number): TranscriptRecord[] {
  const tail = reader.read(path, initialSize, Math.max(0, size - initialSize)).toString('utf8');
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
  current: ClaudeTranscriptSnapshot,
): string[] {
  const normalizedResume = resumeSession?.toLowerCase() ?? null;
  if (normalizedResume) return [join(directory, `${normalizedResume}.jsonl`)];
  return [...current.keys()].filter(path => path === join(directory, basename(path))
    && UUID_FILE.test(basename(path)) && !snapshot.has(path));
}

function hasCompletionMarker(
  directory: string,
  snapshot: ClaudeTranscriptSnapshot,
  marker: string,
  resumeSession: string | null | undefined,
  reader: ClaudeTranscriptReader,
  current: ClaudeTranscriptSnapshot,
): boolean {
  for (const path of candidateTranscriptPaths(directory, snapshot, resumeSession, current)) {
    const size = current.get(path) ?? 0;
    const initialSize = snapshot.get(path) ?? 0;
    if (size <= initialSize) continue;
    // The marker is at the end of the terminal text. A small tail probe avoids
    // reparsing a growing multi-megabyte transcript four times per second.
    const start = Math.max(initialSize, size - 128 * 1024);
    const buffer = reader.read(path, start, size - start);
    if (buffer.includes(Buffer.from(marker))
      && buffer.includes(Buffer.from('"stop_reason":"end_turn"'))) return true;
  }
  return false;
}

export function recoverClaudeTerminalResult({ directory, snapshot, marker, model,
  pricingRates = null, resumeSession = null, reader,
  currentSnapshot = reader.snapshot() }:
  RecoverClaudeTerminalOptions): RecoveredClaudeTerminalResult | null {
  if (!model) throw new Error('terminal recovery model is required');
  const requestedRates = pricingRates === null ? null
    : validatePricingRates(pricingRates, { at: 'terminal recovery pricing rates' });
  const normalizedResume = resumeSession?.toLowerCase() ?? null;
  const candidates = candidateTranscriptPaths(directory, snapshot, resumeSession, currentSnapshot);
  const matches: RecoveredClaudeTerminalResult[] = [];
  for (const path of candidates) {
    const initialSize = snapshot.get(path) ?? 0;
    const size = currentSnapshot.get(path) ?? 0;
    if (size <= initialSize) continue;
    const sessionId = basename(path).match(UUID_FILE)?.[1];
    if (!sessionId || (normalizedResume && sessionId.toLowerCase() !== normalizedResume)) continue;
    const mainRecords = addedRecords(reader, path, initialSize, size)
      .filter((record): record is AssistantUsageRecord => isAssistantUsageRecord(record)
        && record.isSidechain !== true && record.sessionId === sessionId);
    const terminal = mainRecords.at(-1);
    if (!terminal || terminal.message.stop_reason !== 'end_turn'
      || !transcriptContent(terminal.message).some(content => content.type === 'text'
        && markerPresent(String(content.text ?? ''), marker))) continue;
    const records = [...mainRecords];
    const nestedRoot = join(directory, sessionId);
    for (const [nestedPath, nestedSize] of currentSnapshot) {
      if (!nestedPath.startsWith(`${nestedRoot}${sep}`)) continue;
      records.push(...addedRecords(reader, nestedPath, snapshot.get(nestedPath) ?? 0, nestedSize)
        .filter(isAssistantUsageRecord));
    }
    const billed = new Map<string, AssistantUsageRecord>();
    for (const record of records) {
      const requestId = record.requestId ?? record.message.id ?? record.uuid;
      if (typeof requestId !== 'string' || !requestId) {
        throw Object.assign(new Error('terminal transcript usage has no stable request ID'),
          { code: 'CLAUDE_TERMINAL_RECOVERY_UNAVAILABLE' });
      }
      const prior = billed.get(requestId);
      if (prior && JSON.stringify([prior.message.model, normalizeClaudeUsage(prior.message.usage)])
        !== JSON.stringify([record.message.model, normalizeClaudeUsage(record.message.usage)])) {
        throw Object.assign(new Error(`terminal transcript usage changed for request ${requestId}`),
          { code: 'CLAUDE_TERMINAL_RECOVERY_UNAVAILABLE' });
      }
      if (!prior) billed.set(requestId, record);
    }
    if (!billed.size) throw new Error('terminal transcript has no billable assistant usage');
    const usage = { input_tokens: 0, output_tokens: 0,
      cache_creation_input_tokens: 0, cache_read_input_tokens: 0 };
    let totalCostUsd = 0;
    const pricedModels = new Set<string>();
    for (const record of billed.values()) {
      const actualModel = record.message.model;
      if (typeof actualModel !== 'string' || !actualModel) {
        throw Object.assign(new Error('terminal transcript usage has no model'),
          { code: 'CLAUDE_TERMINAL_RECOVERY_UNAVAILABLE' });
      }
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
  transcriptReader,
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
    let previousSnapshot: ClaudeTranscriptSnapshot | null = null;
    let stopping = false;
    const stop = (reason: string): void => {
      if (stopping) return;
      stopping = true;
      Promise.resolve(terminate ? terminate(child, reason) : child.kill('SIGTERM'))
        .catch(() => child.kill('SIGKILL'));
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
      if (recoveryError) return;
      try {
        const currentSnapshot = transcriptReader.snapshot();
        const changed = previousSnapshot === null || currentSnapshot.size !== previousSnapshot.size
          || [...currentSnapshot].some(([path, size]) => previousSnapshot!.get(path) !== size);
        previousSnapshot = currentSnapshot;
        if (!hasCompletionMarker(transcriptDirectory, transcriptSnapshot, marker, resumeSession,
          transcriptReader, currentSnapshot)) {
          terminalSeenAt = null;
          return;
        }
        if (terminalSeenAt === null || changed) terminalSeenAt = Date.now();
        if (Date.now() - terminalSeenAt < exitGraceMs) return;
        const found = recoverClaudeTerminalResult({ directory: transcriptDirectory,
          snapshot: transcriptSnapshot, marker, model, pricingRates, resumeSession,
          reader: transcriptReader, currentSnapshot });
        if (!found) { terminalSeenAt = null; return; }
        recovered = found;
        stop('terminal-transcript');
      } catch (value) {
        recoveryError = value;
        if (isJsonObject(value) && value.code === 'CLAUDE_TERMINAL_RECOVERY_UNAVAILABLE') {
          stop('terminal-recovery-unavailable');
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
      let stderrText = Buffer.concat(stderr).toString('utf8');
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
      if (recoveryError && !error && status !== 0) {
        if (isJsonObject(recoveryError) && recoveryError.code === 'CLAUDE_TERMINAL_RECOVERY_UNAVAILABLE') {
          error = recoveryError;
        }
        else stderrText += `\nTranscript fallback unavailable: ${recoveryError instanceof Error
          ? recoveryError.message : String(recoveryError)}\n`;
      }
      resolve({ status, signal, stdout: stdoutText, stderr: stderrText, error });
    });
  });
}
