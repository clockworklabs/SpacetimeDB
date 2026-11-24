import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

import { hiddenPathDirectories, pathDirectoryCheck } from '../container/coding-providers.js';
import { DEFAULT_BUILD_IMAGE } from '../src/composition/product-config.js';

const [hidden] = hiddenPathDirectories('openai');
const check = pathDirectoryCheck('openai');
const image = process.env.STACK_BENCH_BUILD_IMAGE ?? DEFAULT_BUILD_IMAGE;

const run = (mounts: string[], entrypoint: string, args: readonly string[]) => spawnSync('docker', ['run', '--rm',
  '--read-only', '--network', 'none', '--tmpfs', '/tmp', ...mounts, '--user', '10001:10001', '-e', 'HOME=/tmp',
  '--entrypoint', entrypoint, image, ...args], { encoding: 'utf8', timeout: 120_000 });

// The stand-in model asks for one shell command, then reports the command's output back.
const fixture = String.raw`
import { spawn } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { createServer } from 'node:http';
mkdirSync('/tmp/.codex');
let calls = 0, output = null;
const command = 'command -v rg || echo RG_MISSING; echo "PATH=$PATH"';
function stream(response, item) {
  response.writeHead(200, { 'content-type': 'text/event-stream' });
  const emit = value => response.write('data: ' + JSON.stringify(value) + '\n\n');
  emit({ type: 'response.created', response: { id: 'resp_' + calls, object: 'response', status: 'in_progress', output: [] } });
  emit({ type: 'response.output_item.added', output_index: 0, item });
  emit({ type: 'response.output_item.done', output_index: 0, item });
  emit({ type: 'response.completed', response: { id: 'resp_' + calls, object: 'response', status: 'completed',
    model: 'gpt-5.3-codex', output: [item], usage: { input_tokens: 10, output_tokens: 10, total_tokens: 20,
      input_tokens_details: { cached_tokens: 0 } } } });
  response.end();
}
const server = createServer(async (request, response) => {
  const chunks = []; for await (const chunk of request) chunks.push(chunk);
  if (!request.url.startsWith('/v1/responses')) { response.writeHead(404); response.end(); return; }
  calls++;
  if (calls === 1) {
    stream(response, { type: 'function_call', id: 'fc_path', call_id: 'call_path', name: 'exec_command',
      arguments: JSON.stringify({ cmd: command }) });
    return;
  }
  const input = JSON.parse(Buffer.concat(chunks).toString()).input;
  output = input.filter(item => item.type === 'function_call_output').map(item => JSON.stringify(item.output)).join('\n');
  stream(response, { type: 'message', id: 'msg_final', role: 'assistant',
    content: [{ type: 'output_text', text: 'DONE', annotations: [] }] });
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const child = spawn('codex', ['exec', '--json', '--skip-git-repo-check', '--dangerously-bypass-approvals-and-sandbox',
  '--ignore-user-config', '--ignore-rules', '--model', 'gpt-5.3-codex', '-c', 'model_reasoning_effort="low"',
  '-c', 'model_provider="model_proxy"', '-c', 'web_search="disabled"', '-c', 'features.multi_agent=false',
  '-c', 'model_providers.model_proxy.name="Model API"',
  '-c', 'model_providers.model_proxy.base_url="http://127.0.0.1:' + server.address().port + '/v1"',
  '-c', 'model_providers.model_proxy.env_key="MODEL_PROXY_TOKEN"', '-c', 'model_providers.model_proxy.wire_api="responses"',
  '-c', 'model_providers.model_proxy.request_max_retries=0', '-c', 'model_providers.model_proxy.stream_max_retries=0', '-'],
{ cwd: '/tmp', env: { ...process.env, CODEX_HOME: '/tmp/.codex', MODEL_PROXY_TOKEN: 'fixture-not-a-real-key' },
  stdio: ['pipe', 'pipe', 'pipe'] });
let err = '';
child.stderr.on('data', chunk => { err += chunk; });
child.stdin.end('Run the command.');
const code = await new Promise(resolve => child.on('close', resolve));
server.close();
console.log(JSON.stringify({ code, output, err: err.slice(-1000) }));
`;

const agentCommand = (mounts: string[]): { code: number; output: string | null; err: string } => {
  const result = run(mounts, 'node', ['--input-type=module', '-e', fixture]);
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout.trim().split('\n').at(-1)!);
};

test('the build image\'s Codex adds the hidden directory to PATH, and the check accepts it', () => {
  assert(hidden, 'the Codex provider hides its PATH directory');
  const [entrypoint, ...args] = check;
  const checked = run([], entrypoint!, args);
  assert.equal(checked.status, 0, checked.stderr);
});

test('a command the agent runs through Codex finds no rg once the directory is hidden', () => {
  const exposed = agentCommand([]);
  assert.equal(exposed.code, 0, exposed.err);
  assert.match(exposed.output ?? '', new RegExp(`${hidden}/rg`), 'without the mask Codex lends the agent its rg');
  const masked = agentCommand(['--tmpfs', `${hidden}:ro,nosuid,nodev,size=4k`]);
  assert.equal(masked.code, 0, masked.err);
  assert.match(masked.output ?? '', /RG_MISSING/);
  assert.match(masked.output ?? '', new RegExp(`PATH=[^"]*${hidden}`), 'Codex still adds the hidden directory');
});
