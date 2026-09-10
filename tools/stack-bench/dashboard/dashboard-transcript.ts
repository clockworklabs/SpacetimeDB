import { sha256 } from '../src/evidence/provenance.js';
import { loadTrack, workDirFor } from '../src/composition/tracks.js';
import { execFile } from 'node:child_process';
import { open, readdir, realpath, stat } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { CODING_PROVIDERS } from '../container/coding-providers.js';
import { CONTAINER_CLAUDE_TRANSCRIPT_READ } from '../container/claude-transcript-reader.js';
import { AGENT_ADAPTER_REGISTRY } from '../src/agents/agent-adapters.js';
import type { PublicBackendLease } from '../src/runtime/backend-lease.js';
import { readArtifactPayload } from '../src/evidence/artifacts.js';
import { codingContainerAgentExecOptions } from '../src/runtime/coding-container-policy.js';
import { inspectBuildContainer } from '../src/stacks/hosted-lifecycle.js';
import { redactCredentials } from '../src/evidence/diagnostic-sanitizer.js';

export interface TranscriptMessage { id: string; role: string; text: string; tool: boolean }
export interface TranscriptPage {
  sessions: Array<{ id: string; label: string }>;
  session: string;
  before: number | null;
  messages: TranscriptMessage[];
  skipped: number;
}
const record = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);

export function transcriptMessages(text: string): { messages: TranscriptMessage[]; skipped: number } {
  const messages: TranscriptMessage[] = [];
  let skipped = 0;
  let eventId = '';
  let blockIndex = 0;
  const add = (role: string, value: unknown, tool = false) => {
    if (typeof value === 'string' && value.trim()) messages.push({ id: `${eventId}-${blockIndex++}`, role,
      text: redactCredentials(value), tool });
  };
  for (const line of text.split('\n').filter(line => line.trim())) {
    eventId = sha256(line);
    blockIndex = 0;
    let event: unknown;
    try { event = JSON.parse(line); } catch { skipped++; continue; }
    if (!record(event)) continue;
    const message = record(event.message) ? event.message
      : event.type === 'response_item' && record(event.payload) ? event.payload : null;
    if (message) {
      const role = String(message.role ?? event.type ?? 'Agent');
      if (typeof message.content === 'string') add(role, message.content);
      if (Array.isArray(message.content)) for (const block of message.content) {
        if (!record(block)) continue;
        if (['text', 'input_text', 'output_text'].includes(String(block.type))) add(role, block.text);
        if (block.type === 'tool_use') add(String(block.name ?? 'Tool'), JSON.stringify(block.input, null, 2), true);
        if (block.type === 'tool_result') add('Tool result', typeof block.content === 'string'
          ? block.content : JSON.stringify(block.content, null, 2), true);
      }
      if (message.type === 'function_call') add(String(message.name ?? 'Tool'), message.arguments, true);
      if (message.type === 'function_call_output') add('Tool result', message.output, true);
    }
    if (event.type === 'item.completed' && record(event.item)) {
      const item = event.item;
      if (item.type === 'agent_message') add('assistant', item.text);
      if (item.type === 'command_execution') add('Command', `${item.command ?? ''}\n${item.aggregated_output ?? ''}`, true);
      if (item.type === 'file_change') add('File changes', JSON.stringify(item.changes, null, 2), true);
    }
  }
  return { messages, skipped };
}

export interface TranscriptFile {
  id: string;
  label: string;
  size: number;
  modified: number;
  read(start: number, count: number): Promise<Buffer>;
}
const pendingReads = new Map<string, Promise<Buffer>>();
function dockerRead(args: string[]): Promise<Buffer> {
  const key = JSON.stringify(args);
  const pending = pendingReads.get(key);
  if (pending) return pending;
  const result = new Promise<Buffer>((resolve, reject) => {
    execFile('docker', args, { timeout: 5000, maxBuffer: 2 * 1024 * 1024, encoding: 'buffer' },
      (error, stdout) => error ? reject(error) : resolve(stdout));
  }).finally(() => pendingReads.delete(key));
  pendingReads.set(key, result);
  return result;
}

// Reads only this attempt's transcript mounts. Never scans another account's sessions.
export async function attemptTranscriptFiles(executions: Array<{ directory: string; label: string }>,
  adapterId: string): Promise<TranscriptFile[]> {
  const provider = AGENT_ADAPTER_REGISTRY.get(adapterId).provider;
  if (!provider || !(provider in CODING_PROVIDERS)) return [];
  const config = CODING_PROVIDERS[provider as keyof typeof CODING_PROVIDERS];
  const files: TranscriptFile[] = [];
  for (const execution of executions) {
    const leasePath = join(execution.directory, 'backend-lease.json');
    if (!existsSync(leasePath)) continue;
    const lease = readArtifactPayload<PublicBackendLease>(leasePath, { expectedKind: 'backend_lease_evidence' });
    const root = config.projects(join(workDirFor(loadTrack(lease.track), lease.backend, lease.runIndex, lease.runId), 'app'));
    let remote: ((name: string, start: number, count: number) => Promise<Buffer>) | null = null;
    if (lease.state === 'active' && lease.resources.buildContainer?.owned) {
      const actual = await dockerRead(['inspect', '--format', '{{.Id}}', lease.resources.buildContainer.name]);
      const container = inspectBuildContainer(lease, () => actual.toString());
      remote = (name, start, count) => dockerRead(['exec', ...codingContainerAgentExecOptions(),
          container.id, 'node', '-e', CONTAINER_CLAUDE_TRANSCRIPT_READ,
          config.containerTranscripts, name, String(start), String(count)]);
    }
    let entries: Array<[string, number, number]>;
    if (remote) entries = JSON.parse((await remote('', 0, 0)).toString()) as Array<[string, number, number]>;
    else {
      if (!existsSync(root)) continue;
      const resolvedRoot = await realpath(root);
      entries = [];
      for (const entry of await readdir(root, { recursive: true, withFileTypes: true })) {
        if (!entry.isFile() || !entry.name.endsWith('.jsonl')) continue;
        const path = join(entry.parentPath, entry.name);
        if (!(await realpath(path)).startsWith(resolvedRoot + sep)) continue;
        const info = await stat(path);
        entries.push([relative(root, path), info.size, info.mtimeMs]);
      }
    }
    for (const [name, size, modified] of entries) {
      const reader = remote;
      files.push({ id: Buffer.from(`${execution.label}/${name}`).toString('base64url'),
        label: `${execution.label} / ${new Date(modified).toISOString().replace('T', ' ').slice(0, 16)} UTC`, size, modified,
        read: async (start, count) => {
          if (!Number.isSafeInteger(start) || start < 0 || !Number.isSafeInteger(count)
            || count < 0 || count > 256 * 1024) throw new Error('Invalid transcript range');
          if (reader) return reader(name, start, count);
          const path = join(root, name);
          if (!(await realpath(path)).startsWith(await realpath(root) + sep)) {
            throw new Error('transcript is outside the attempt directory');
          }
          const fd = await open(path, 'r'), buffer = Buffer.alloc(count);
          try { return buffer.subarray(0, (await fd.read(buffer, 0, count, start)).bytesRead); }
          finally { await fd.close(); }
        } });
    }
  }
  return files.sort((a, b) => a.modified - b.modified);
}
export async function readAttemptTranscript(executions: Array<{ directory: string; label: string }>,
  adapterId: string, session = '', before?: number): Promise<TranscriptPage> {
  const files = await attemptTranscriptFiles(executions, adapterId);
  const file = (session ? files.find(file => file.id === session) : files.at(-1));
  if (session && !file) throw new Error('Transcript session not found');
  if (!file) return { sessions: [], session: '', before: null, messages: [], skipped: 0 };
  const end = Math.min(before ?? file.size, file.size);
  const start = Math.max(0, end - 256 * 1024);
  const bytes = await file.read(start, end - start);
  const first = start ? bytes.indexOf(10) + 1 : 0;
  const last = bytes.lastIndexOf(10);
  const content = last >= first ? bytes.subarray(first, last + 1).toString('utf8') : '';
  return { sessions: files.map(({ id, label }) => ({ id, label })), session: file.id,
    before: start > 0 ? start + first : null, ...transcriptMessages(content) };
}
