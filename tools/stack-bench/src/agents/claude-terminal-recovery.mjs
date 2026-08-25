import { spawn } from 'node:child_process';
import { closeSync, existsSync, openSync, readFileSync, readSync,
  readdirSync, statSync } from 'node:fs';
import { basename, join, sep } from 'node:path';

import { claudeRatesForModel, normalizeClaudeUsage,
  priceClaudeUsage } from '../evidence/claude-usage-cost.mjs';

const UUID_FILE = /^([0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})\.jsonl$/i;

export function snapshotClaudeTranscripts(directory) {
  const snapshot = new Map();
  if (!existsSync(directory)) return snapshot;
  (function walk(path) {
    for (const entry of readdirSync(path, { withFileTypes: true })) {
      const child = join(path, entry.name);
      if (entry.isDirectory()) walk(child);
      else if (entry.name.endsWith('.jsonl')) snapshot.set(child, statSync(child).size);
    }
  })(directory);
  return snapshot;
}

function markerPresent(text, marker) {
  return new RegExp(`(?:^|\\s)${marker}(?=\\s|$)`).test(text);
}

function addedRecords(path, initialSize) {
  const value = readFileSync(path, 'utf8');
  // Transcript records are ASCII JSON around UTF-8 string content. Slicing the
  // byte buffer, not the JavaScript string, keeps the saved offset exact.
  const tail = Buffer.from(value).subarray(initialSize).toString('utf8');
  return tail.split(/\r?\n/).filter(Boolean).flatMap(line => {
    try { return [JSON.parse(line)]; } catch { return []; }
  });
}

export function parseCompleteClaudeCliResult(raw) {
  const value = String(raw ?? '').trim();
  if (!value) return null;
  let parsed = null;
  try { parsed = JSON.parse(value); } catch {
    for (const line of value.split(/\r?\n/).reverse()) {
      try { parsed = JSON.parse(line); break; } catch { /* keep looking */ }
    }
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)
    || typeof parsed.is_error !== 'boolean'
    || typeof parsed.session_id !== 'string' || !parsed.session_id
    || typeof parsed.result !== 'string'
    || !Number.isFinite(parsed.total_cost_usd) || parsed.total_cost_usd < 0
    || !Number.isSafeInteger(parsed.num_turns) || parsed.num_turns < 0
    || !parsed.usage || typeof parsed.usage !== 'object' || Array.isArray(parsed.usage)) {
    return null;
  }
  try { normalizeClaudeUsage(parsed.usage); } catch { return null; }
  return parsed;
}

function candidateTranscriptPaths(directory, snapshot, resumeSession) {
  const normalizedResume = resumeSession?.toLowerCase() ?? null;
  if (normalizedResume) return [join(directory, `${normalizedResume}.jsonl`)];
  return existsSync(directory)
    ? readdirSync(directory).filter(name => UUID_FILE.test(name)).map(name => join(directory, name))
      .filter(path => !snapshot.has(path))
    : [];
}

function hasCompletionMarker(directory, snapshot, marker, resumeSession) {
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
  resumeSession = null }) {
  const normalizedResume = resumeSession?.toLowerCase() ?? null;
  const candidates = candidateTranscriptPaths(directory, snapshot, resumeSession);
  const matches = [];
  for (const path of candidates) {
    if (!existsSync(path)) continue;
    const initialSize = snapshot.get(path) ?? 0;
    if (statSync(path).size <= initialSize) continue;
    const sessionId = basename(path).match(UUID_FILE)?.[1];
    if (!sessionId || (normalizedResume && sessionId.toLowerCase() !== normalizedResume)) continue;
    const mainRecords = addedRecords(path, initialSize)
      .filter(record => record?.type === 'assistant' && record.isSidechain !== true
        && record.sessionId === sessionId && record?.message?.usage);
    const terminal = mainRecords.findLast(record => record.message.stop_reason === 'end_turn'
      && (record.message.content ?? []).some(content => content.type === 'text'
        && markerPresent(content.text ?? '', marker)));
    if (!terminal) continue;
    const records = [...mainRecords];
    const nestedRoot = join(directory, sessionId);
    if (existsSync(nestedRoot)) {
      for (const nestedPath of snapshotClaudeTranscripts(nestedRoot).keys()) {
        if (!nestedPath.startsWith(`${nestedRoot}${sep}`)) continue;
        records.push(...addedRecords(nestedPath, snapshot.get(nestedPath) ?? 0)
          .filter(record => record?.type === 'assistant' && record?.message?.usage));
      }
    }
    const billed = new Map();
    for (const record of records) {
      const requestId = record.requestId ?? record.message?.id ?? record.uuid;
      if (requestId && !billed.has(requestId)) billed.set(requestId, record);
    }
    if (!billed.size) throw new Error('terminal transcript has no billable assistant usage');
    const usage = { input_tokens: 0, output_tokens: 0,
      cache_creation_input_tokens: 0, cache_read_input_tokens: 0 };
    let totalCostUsd = 0;
    const pricedModels = new Set();
    for (const record of billed.values()) {
      const actualModel = record.message?.model ?? model;
      const rates = claudeRatesForModel(actualModel);
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
    const result = (terminal.message.content ?? [])
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
  exitGraceMs = 15_000, pollMs = 250, maxBuffer = 256 * 1024 * 1024,
  terminate }) {
  return new Promise(resolve => {
    const child = spawn(command, args, { env, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    const stdout = [];
    const stderr = [];
    let stdoutBytes = 0;
    let stderrBytes = 0;
    let error = null;
    let recovered = null;
    let recoveryError = null;
    let terminalSeenAt = null;
    let stopping = false;
    const stop = reason => {
      if (stopping) return;
      stopping = true;
      Promise.resolve(terminate?.(child, reason)).catch(() => child.kill('SIGKILL'));
    };
    const append = (target, chunk, stdoutStream) => {
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
          snapshot: transcriptSnapshot, marker, model, resumeSession });
        if (!found) return;
        if (terminalSeenAt === null) terminalSeenAt = Date.now();
        if (Date.now() - terminalSeenAt >= exitGraceMs) {
          recovered = found;
          stop('terminal-transcript');
        }
      } catch (value) {
        recoveryError = value;
        if (value?.code === 'CLAUDE_TERMINAL_RECOVERY_UNAVAILABLE') terminalSeenAt = Date.now();
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
      if (recovered) {
        const cliResult = parseCompleteClaudeCliResult(stdoutText);
        if (cliResult) {
          const terminalRecovery = { schemaVersion: 1, kind: 'terminal-process', marker,
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
