import { execFile } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolveContainerAuth } from '../../container/container-auth.js';
import { AGENT_ADAPTER_REGISTRY } from './agent-adapters.js';
import { listCredentialProfiles, resolveExecutionCredentials } from './credential-profiles.js';

const codexList = `const fs=require('node:fs'),{spawn}=require('node:child_process');
const home='/tmp/stack-bench-codex';fs.mkdirSync(home,{recursive:true,mode:0o700});
const token=fs.readFileSync(0,'utf8').trim();
const child=spawn('codex',['app-server'],{env:{...process.env,CODEX_HOME:home,CODEX_ACCESS_TOKEN:token},stdio:['pipe','pipe','ignore']});
const send=(id,method,params)=>child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\\n');
let buffer='',done=false;const timer=setTimeout(()=>{child.kill();process.exit(2)},15000);
child.stdout.on('data',chunk=>{buffer+=chunk;let end;
while((end=buffer.indexOf('\\n'))>=0){const line=buffer.slice(0,end);buffer=buffer.slice(end+1);let message;
try{message=JSON.parse(line)}catch{continue}
if(message.id===1)send(2,'model/list',{});
if(message.id===2){done=true;clearTimeout(timer);console.log(JSON.stringify(message.result?.data??[]));child.kill();}}});
child.on('exit',()=>{if(!done)process.exitCode=2});
send(1,'initialize',{clientInfo:{name:'stack-bench',version:'1.0.0'},capabilities:{}});`;

function secret(env: NodeJS.ProcessEnv, variable: string, fallback?: string): string {
  let value = env[variable] ?? '';
  if (!value && (env[`${variable}_FILE`] || fallback)) {
    try { value = readFileSync(env[`${variable}_FILE`] ?? fallback!, 'utf8').trim(); }
    catch { throw new Error(`${variable} is not configured for model discovery`); }
  }
  if (!value) throw new Error(`${variable} is not configured for model discovery`);
  return value;
}

function models(value: unknown): Array<{ id: string }> {
  if (!Array.isArray(value)) throw new Error('Provider returned an invalid model list');
  return value.flatMap(item => {
    if (!item || typeof item !== 'object') return [];
    const row = item as Record<string, unknown>;
    if (typeof row.id !== 'string' || !row.id || row.id.length > 128) return [];
    return [{ id: row.id }];
  }).slice(0, 1000);
}

/** Read provider metadata only. It does not issue a model request or change run evidence. */
export async function discoverModels(adapter: string, profileId?: string,
  env: NodeJS.ProcessEnv = process.env): Promise<{ adapter: string; profile: string | null;
    models: Array<{ id: string }> }> {
  const provider = AGENT_ADAPTER_REGISTRY.get(adapter).provider;
  if (!provider) throw new Error(`${adapter} has no provider model catalog`);
  const profiles = listCredentialProfiles(env).filter(p => p.provider === provider);
  if (!profileId && profiles.length > 1) throw new Error(`Choose an account for ${adapter} before loading models`);
  const selectedId = profileId ?? profiles[0]?.id;
  const resolved = resolveExecutionCredentials(adapter, '', selectedId
    ? { adapters: { [adapter]: selectedId } } : {}, env);
  const source = resolved.env;
  const mode = resolved.assignment?.mode ?? (provider === 'openrouter' ? 'api-key'
    : ['openai-api-key', 'anthropic-api-key'].includes(env.STACK_BENCH_AGENT_AUTH ?? '') ? 'api-key' : 'subscription-token');
  let data: unknown;
  if (provider === 'openai' && mode === 'subscription-token') {
    const authFile = source.CODEX_AUTH_FILE ?? source.STACK_BENCH_CODEX_AUTH_FILE;
    const image = env.STACK_BENCH_BUILD_IMAGE;
    if (!authFile || !image || !/^sha256:[a-f0-9]{64}$/.test(image)) {
      throw new Error('Codex account discovery requires its auth file and the pinned build image');
    }
    const token = resolveContainerAuth({ provider: 'openai',
      env: { ...source, CODEX_AUTH_FILE: authFile } }).credential;
    try {
      const stdout = await new Promise<string>((resolve, reject) => {
        const child = execFile('docker', ['run', '--rm', '-i', '--network', 'bridge', '--read-only',
          '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges:true', '--pids-limit', '64',
          '--memory', '512m', '--tmpfs', '/tmp:mode=1777', '--entrypoint', 'node', image,
          '-e', codexList], { timeout: 25_000, maxBuffer: 1_000_000, windowsHide: true },
        (error, output) => error ? reject(error) : resolve(output));
        child.stdin?.on('error', () => {});
        child.stdin?.end(token);
      });
      data = JSON.parse(stdout);
    } catch { throw new Error('Codex could not list models for this account'); }
  } else {
    const variable = provider === 'anthropic' ? mode === 'api-key' ? 'ANTHROPIC_API_KEY' : 'CLAUDE_CODE_OAUTH_TOKEN'
      : provider === 'openrouter' ? 'OPENROUTER_API_KEY' : 'OPENAI_API_KEY';
    const fallback = provider === 'anthropic'
      ? source[mode === 'api-key' ? 'STACK_BENCH_ANTHROPIC_API_KEY_FILE' : 'STACK_BENCH_CLAUDE_OAUTH_TOKEN_FILE']
      : source[provider === 'openrouter' ? 'STACK_BENCH_OPENROUTER_API_KEY_FILE' : 'STACK_BENCH_OPENAI_API_KEY_FILE'];
    const token = secret(source, variable, fallback);
    const url = provider === 'anthropic' ? 'https://api.anthropic.com/v1/models?limit=1000'
      : provider === 'openrouter' ? 'https://openrouter.ai/api/v1/models/user' : 'https://api.openai.com/v1/models';
    const response = await fetch(url, { headers: { authorization: `Bearer ${token}`,
      ...(provider === 'anthropic' ? { 'anthropic-version': '2023-06-01' } : {}) },
    signal: AbortSignal.timeout(15_000) });
    if (!response.ok) throw new Error(`${provider} model list failed (HTTP ${response.status})`);
    data = (await response.json() as { data?: unknown }).data;
  }
  return { adapter, profile: selectedId ?? null, models: models(data) };
}
