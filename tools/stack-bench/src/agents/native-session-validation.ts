import { lstatSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

type Json = Record<string, unknown>;
const record = (value: unknown): value is Json => value !== null && typeof value === 'object' && !Array.isArray(value);

/** Refuse incomplete tool exchanges rather than replaying a possibly completed tool. */
export function validateClaudeContinuationTranscript(text: string, sessionId: string): void {
  const rows = text.split(/\r?\n/).filter(value => value.trim()).map(line => JSON.parse(line) as unknown);
  const parents = new Map<string, string | null>();
  const calls = new Map<string, string>();
  const results = new Map<string, string>();
  let root: string | null = null;
  let assistant = false;
  // Claude may persist an assistant record before its user parent. File order is
  // not conversation order; first collect the native graph, then validate it.
  for (const row of rows) {
    if (!record(row)) throw new Error('Invalid native session record');
    if (row.sessionId !== undefined && row.sessionId !== sessionId) throw new Error('Native session identity changed');
    if (row.isSidechain === true) throw new Error('Sidechain continuation is not qualified');
    const message = row.type === 'user' || row.type === 'assistant';
    if (typeof row.uuid !== 'string' || !row.uuid) {
      if (message) throw new Error('Native conversation is missing parent history');
      continue;
    }
    if (!(row.parentUuid === null || typeof row.parentUuid === 'string')) {
      if (message) throw new Error('Native conversation is missing parent history');
      continue;
    }
    // Compaction starts a new replay segment, but retains its historical parent.
    // Follow that link for integrity checks; never rewrite the native transcript.
    const compacted = row.type === 'system' && row.subtype === 'compact_boundary';
    if (compacted && (row.parentUuid !== null || typeof row.logicalParentUuid !== 'string'
      || !row.logicalParentUuid || !rows.some(summary => record(summary) && summary.type === 'user'
        && summary.parentUuid === row.uuid && summary.isCompactSummary === true
        && record(summary.message) && typeof summary.message.content === 'string'
        && summary.message.content.trim()))) throw new Error('Native compaction is missing history or summary');
    const parent = compacted ? row.logicalParentUuid as string : row.parentUuid;
    if (parents.has(row.uuid) && parents.get(row.uuid) !== parent) throw new Error('Native parent history conflicts');
    parents.set(row.uuid, parent);
    if (parent === null) {
      if ((root !== null && root !== row.uuid) || row.type !== 'user') throw new Error('Native conversation has disconnected roots');
      const content = record(row.message) ? row.message.content : undefined;
      const task = typeof content === 'string' ? content.trim().length > 0
        : Array.isArray(content) && content.some(block => record(block)
          && block.type === 'text' && typeof block.text === 'string' && block.text.trim().length > 0);
      if (!task) throw new Error('Native conversation is missing its original task');
      root = row.uuid;
    }
    if (row.type === 'assistant') assistant = true;
    if (!record(row.message) || !Array.isArray(row.message.content)) continue;
    for (const block of row.message.content) {
      if (!record(block) || !['tool_use', 'tool_result'].includes(String(block.type))) continue;
      const collection = block.type === 'tool_use' ? calls : results;
      const id = block.type === 'tool_use' ? block.id : block.tool_use_id;
      if (typeof id !== 'string' || (collection.has(id) && collection.get(id) !== row.uuid)) {
        throw new Error('Native tool identity is missing or ambiguous');
      }
      collection.set(id, row.uuid);
    }
  }
  if (!root || !assistant) throw new Error('Native conversation has an orphan parent or missing task');
  const connected = new Set<string>([root]);
  for (const id of parents.keys()) {
    const path = new Set<string>();
    let cursor = id;
    while (!connected.has(cursor)) {
      if (path.has(cursor)) throw new Error('Native parent history contains a cycle');
      path.add(cursor);
      const parent = parents.get(cursor);
      if (!parent || !parents.has(parent)) throw new Error('Native conversation has an orphan parent');
      cursor = parent;
    }
    for (const node of path) connected.add(node);
  }
  for (const [id, result] of results) {
    const call = calls.get(id);
    let cursor = parents.get(result);
    while (cursor && cursor !== call) cursor = parents.get(cursor);
    if (!call || cursor !== call) throw new Error('Native tool result has no matching ancestor request');
  }
  if ([...calls.keys()].some(id => !results.has(id))) throw new Error('Native session has unresolved tools');
}

export function validateClaudeNativeSession(directory: string, sessionId: string): void {
  const path = join(directory, `${sessionId}.jsonl`);
  if (!lstatSync(path).isFile()) throw new Error('Native session is missing or is not a regular file');
  validateClaudeContinuationTranscript(readFileSync(path, 'utf8'), sessionId);
}

/** Codex restores the native rollout, not the exported exec event stream. */
export function validateCodexContinuationTranscript(text: string, sessionId: string, model: string): void {
  const rows: unknown[] = text.split(/\r?\n/).filter(line => line.trim()).map(line => JSON.parse(line));
  const first = rows[0];
  if (!record(first) || first.type !== 'session_meta' || !record(first.payload)
    || first.payload.id !== sessionId || first.payload.cwd !== '/app'
    || first.payload.model_provider !== 'model_proxy'
    || !record(first.payload.base_instructions) || typeof first.payload.base_instructions.text !== 'string'
    || !first.payload.base_instructions.text.trim()) throw new Error('Native session identity or base instructions are missing');
  const calls = new Set<string>();
  const results = new Set<string>();
  const settled = (): void => {
    if ([...calls].some(id => !results.has(id))) throw new Error('Native session has unresolved tools');
  };
  const toolExchange = (value: unknown): void => {
    if (!record(value) || typeof value.type !== 'string') throw new Error('Invalid native response item');
    const call = value.type === 'function_call' || value.type === 'custom_tool_call';
    const result = value.type === 'function_call_output' || value.type === 'custom_tool_call_output';
    if (/_call(?:_output)?$/.test(value.type) && !call && !result) throw new Error('Native tool type is not qualified');
    if (!call && !result) return;
    const id = value.call_id;
    if (typeof id !== 'string' || !id || (call ? calls : results).has(id))
      throw new Error('Native tool identity is missing or ambiguous');
    if (result && !calls.has(id)) throw new Error('Native tool result has no matching request');
    (call ? calls : results).add(id);
  };
  let task: string | null = null, context = false;
  const userInputs = new Set<string>();
  for (const row of rows) {
    if (!record(row) || !record(row.payload)) throw new Error('Invalid native session record');
    const value = row.payload;
    if (row.type === 'session_meta') {
      if (value.id !== sessionId || value.forked_from_id || value.parent_thread_id || value.history_base)
        throw new Error('Native fork or external history is not qualified');
    }
    if (row.type === 'event_msg' && value.type === 'item_completed' && record(value.item)
      && value.item.type === 'UserMessage' && task === null) {
      if (!Array.isArray(value.item.content)) throw new Error('Native original task is missing');
      const parts = value.item.content.filter(part => record(part) && part.type === 'text'
        && typeof part.text === 'string').map(part => String(part.text));
      task = parts.join('\n');
      if (!task.trim()) throw new Error('Native original task is empty');
    }
    if (row.type === 'compacted') {
      settled();
      calls.clear(); results.clear();
      // Pinned Codex replaces effective replay history with this array. Legacy
      // summary-only records rebuild user messages plus the summary, without tools.
      if (value.replacement_history !== undefined && value.replacement_history !== null) {
        if (!Array.isArray(value.replacement_history) || !value.replacement_history.length
          || (value.replacement_history_metadata !== undefined && (!Array.isArray(value.replacement_history_metadata)
            || value.replacement_history_metadata.length !== value.replacement_history.length)))
          throw new Error('Native compacted replacement history is invalid');
        for (const item of value.replacement_history) toolExchange(item);
        settled();
      } else if (typeof value.message !== 'string' || !value.message.trim() || value.replacement_history_metadata !== undefined) {
        throw new Error('Native compacted summary is invalid');
      }
    }
    if (row.type === 'turn_context') {
      if (value.model !== model || value.cwd !== '/app') throw new Error('Native model or workspace changed');
      context = true;
    }
    if (row.type === 'response_item') {
      if (value.type === 'message' && value.role === 'user' && Array.isArray(value.content)) {
        for (const block of value.content) if (record(block) && block.type === 'input_text'
          && typeof block.text === 'string') userInputs.add(block.text);
      }
      toolExchange(value);
    }
  }
  if (!task || !userInputs.has(task) || !context) throw new Error('Native conversation is missing its task or turn context');
  settled();
}

export function validateCodexNativeSession(directory: string, sessionId: string, model: string): void {
  const paths = readdirSync(directory, { recursive: true, withFileTypes: true })
    .filter(entry => {
      if (entry.isSymbolicLink()) throw new Error('Native session contains a symbolic link');
      return entry.isFile() && entry.name.startsWith('rollout-') && entry.name.endsWith(`-${sessionId}.jsonl`);
    }).map(entry => join(entry.parentPath, entry.name));
  if (paths.length !== 1) throw new Error('Native rollout is missing or ambiguous');
  validateCodexContinuationTranscript(readFileSync(paths[0]!, 'utf8'), sessionId, model);
}
