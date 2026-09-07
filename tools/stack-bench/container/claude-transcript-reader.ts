import { execFileSync } from 'node:child_process';
import { isAbsolute, join, relative, sep } from 'node:path';
import type { ClaudeTranscriptReader } from '../src/agents/claude-terminal-recovery.js';
import { CODING_CONTAINER_AGENT, codingContainerAgentExecOptions }
  from '../src/runtime/coding-container-policy.js';

// Read as the transcript owner. Claude creates private files while the controller
// has no DAC override; the final transcript handback cannot serve a live reader.
export const CONTAINER_CLAUDE_TRANSCRIPT_READ = `
const fs = require('node:fs'), path = require('node:path');
const [root, name, offset, count] = process.argv.slice(1);
if (name === '') {
  const files = fs.readdirSync(root, { recursive: true, withFileTypes: true })
    .filter(entry => entry.isFile() && entry.name.endsWith('.jsonl'))
    .map(entry => path.join(entry.parentPath, entry.name))
    .map(file => [path.relative(root, file), fs.statSync(file).size]);
  process.stdout.write(JSON.stringify(files));
} else {
  const file = path.resolve(root, name), resolvedRoot = fs.realpathSync(root);
  if (!file.endsWith('.jsonl') || !fs.realpathSync(file).startsWith(resolvedRoot + path.sep)) {
    throw new Error('transcript is outside the attempt directory');
  }
  const start = Number(offset), length = Number(count);
  if (!Number.isSafeInteger(start) || start < 0 || !Number.isSafeInteger(length) || length < 0) {
    throw new Error('invalid transcript range');
  }
  const buffer = Buffer.alloc(length), fd = fs.openSync(file, 'r');
  try { process.stdout.write(buffer.subarray(0, fs.readSync(fd, buffer, 0, length, start))); }
  finally { fs.closeSync(fd); }
}
`;

export function containerClaudeTranscriptReader(containerId: string, directory: string,
  env: NodeJS.ProcessEnv): ClaudeTranscriptReader {
  if (!/^[a-f0-9]{64}$/.test(containerId)) throw new Error('transcript reader requires an exact container ID');
  const root = `${CODING_CONTAINER_AGENT.home}/.claude/projects/-app`;
  const read = (name: string, start = 0, length = 0): Buffer => execFileSync('docker', [
    'exec', ...codingContainerAgentExecOptions(), containerId, 'node', '-e',
    CONTAINER_CLAUDE_TRANSCRIPT_READ, root, name, String(start), String(length),
  ], { env, timeout: 5_000, maxBuffer: 256 * 1024 * 1024 });
  return {
    snapshot() {
      const entries: [string, number][] = JSON.parse(read('').toString('utf8'));
      return new Map(entries.map(([name, size]) => [join(directory, name), size]));
    },
    read(path, start, length) {
      const name = relative(directory, path);
      if (!name || isAbsolute(name) || name.split(sep).includes('..')) {
        throw new Error('transcript is outside the attempt directory');
      }
      return read(name.split(sep).join('/'), start, length);
    },
  };
}
