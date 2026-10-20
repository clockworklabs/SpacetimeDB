import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { DEFAULT_BUILD_IMAGE } from '../src/composition/product-config.js';
import { captureNativeContinuation } from '../src/agents/provider-native-continuation.js';
import { CODING_PROVIDERS } from '../container/coding-providers.js';
import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { sha256 } from '../src/evidence/provenance.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { campaignLockTransaction, controllerInstance } from '../src/campaigns/campaign-lock.js';
import { claimNextAttempt, initializeCampaignDirectory, writeCampaignState } from '../src/campaigns/campaign-scheduler.js';
import { persistCampaignProviderInvocation, readCampaignProviderContinuationStatus,
  requestCampaignProviderContinuation, waitForCampaignProviderContinuation }
  from '../src/campaigns/campaign-provider-continuation.js';

// Only the pinned CLI runs. All model responses come from this local fake server.
const fixture = String.raw`
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { DatabaseSync } from 'node:sqlite';
let allowed = false, wrote = false, calls = 0, resumedContext = false;
const command = "node --experimental-sqlite -e 'const fs=require(\"node:fs\");const {DatabaseSync}=require(\"node:sqlite\");fs.writeFileSync(\"/app/candidate.txt\",\"retained\");const db=new DatabaseSync(\"/app/state.sqlite\");db.exec(\"CREATE TABLE state(value TEXT); INSERT INTO state VALUES (\"+String.fromCharCode(39)+\"retained\"+String.fromCharCode(39)+\")\");console.log(\"STATE_WRITTEN\")'";
function stream(response, content, stop) {
  response.writeHead(200, {'content-type':'text/event-stream'});
  const emit = value => response.write('event: '+value.type+'\ndata: '+JSON.stringify(value)+'\n\n');
  emit({type:'message_start',message:{id:'msg_'+calls,type:'message',role:'assistant',model:'claude-sonnet-5',content:[],stop_reason:null,stop_sequence:null,usage:{input_tokens:10,output_tokens:1}}});
  if(content.type==='tool_use') {
    emit({type:'content_block_start',index:0,content_block:{type:'tool_use',id:'toolu_state',name:'Bash',input:{}}});
    emit({type:'content_block_delta',index:0,delta:{type:'input_json_delta',partial_json:JSON.stringify({command,description:'Write local application state'})}});
  } else {
    emit({type:'content_block_start',index:0,content_block:{type:'text',text:''}});
    emit({type:'content_block_delta',index:0,delta:{type:'text_delta',text:'CONTINUED_WITH_STATE'}});
  }
  emit({type:'content_block_stop',index:0});
  emit({type:'message_delta',delta:{stop_reason:stop,stop_sequence:null},usage:{output_tokens:10}});
  emit({type:'message_stop'}); response.end();
}
const server=createServer(async(request,response)=>{
  const chunks=[];for await(const chunk of request)chunks.push(chunk);
  if(request.url.includes('count_tokens')) {response.writeHead(200,{'content-type':'application/json'});response.end('{"input_tokens":10}');return;}
  if(!request.url.startsWith('/v1/messages')) {response.writeHead(404);response.end();return;}
  calls++;const body=JSON.parse(Buffer.concat(chunks).toString());
  if(!wrote){wrote=true;stream(response,{type:'tool_use'},'tool_use');return;}
  if(!allowed){response.writeHead(429,{'content-type':'application/json','x-should-retry':'false','retry-after':'0'});response.end(JSON.stringify({type:'error',error:{type:'rate_limit_error',message:'fixture rate limit'}}));return;}
  resumedContext=JSON.stringify(body.messages).includes('STATE_WRITTEN');
  stream(response,{type:'text'},'end_turn');
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const address=server.address();
const env={...process.env,ANTHROPIC_API_KEY:'fixture-not-a-real-key',ANTHROPIC_BASE_URL:'http://127.0.0.1:'+address.port,DISABLE_AUTOUPDATER:'1',CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC:'1',CLAUDE_CODE_MAX_RETRIES:'0',API_MAX_RETRIES:'0'};
function invoke(extra,prompt){return new Promise((resolve,reject)=>{
  const child=spawn('claude',['--print','--output-format','json','--bare','--permission-mode','acceptEdits','--settings',JSON.stringify({permissions:{allow:['Bash']}}),'--effort','low','--model','claude-sonnet-5','--add-dir','/app',...extra],{cwd:'/app',env,stdio:['pipe','pipe','pipe']});
  let out='',err='';child.stdout.on('data',b=>out+=b);child.stderr.on('data',b=>err+=b);
  const timer=setTimeout(()=>{child.kill('SIGKILL');reject(new Error('CLI timeout: '+err.slice(-500)));},45000);
  child.on('error',reject);child.on('close',code=>{clearTimeout(timer);try{resolve({code,result:JSON.parse(out),err});}catch{reject(new Error('CLI invalid output '+out.slice(-500)+' '+err.slice(-500)));}});child.stdin.end(prompt);
});}
function state(){const db=new DatabaseSync('/app/state.sqlite');try{return {file:readFileSync('/app/candidate.txt','utf8'),row:db.prepare('SELECT value FROM state').get().value};}finally{db.close();}}
try {
 const first=await invoke([],'Write the local application state.');
 assert(first.result.is_error,JSON.stringify(first));
 const sessionId=first.result.session_id;
 const path='/home/developer/.claude/projects/-app/'+sessionId+'.jsonl';
 const before=readFileSync(path,'utf8');let stateBefore;
 try {stateBefore=state();} catch(error){throw new Error(String(error)+' transcript: '+before.slice(-5000));}
 console.log(JSON.stringify({paused:true,sessionId,before,stateBefore,firstError:first.result.result}));
 await new Promise(resolve=>process.stdin.once('data',resolve));
 // The host sends input only after the real campaign helper accepts the request.
 allowed=true;
 const second=await invoke(['--resume',sessionId],'Continue.');
 assert.equal(second.result.is_error,false,JSON.stringify(second));
 assert.equal(second.result.session_id,sessionId);
 assert(resumedContext,'native resume lost the tool result');
 const after=readFileSync(path,'utf8'),stateAfter=state();
 assert.deepEqual(stateAfter,stateBefore);assert.equal(stateAfter.row,'retained');
 console.log(JSON.stringify({sessionId,before,after,stateBefore,stateAfter,calls,firstError:first.result.result,second:second.result.result}));
} finally {server.close();}
`;


const codexFixture = String.raw`
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { createServer } from 'node:http';
import { DatabaseSync } from 'node:sqlite';
let allowed=false, wrote=false, calls=0, resumedContext=false, firstRequest;
const command = "node --experimental-sqlite -e 'const fs=require(\"node:fs\");const {DatabaseSync}=require(\"node:sqlite\");fs.writeFileSync(\"/app/candidate.txt\",\"retained\");const db=new DatabaseSync(\"/app/state.sqlite\");db.exec(\"CREATE TABLE state(value TEXT); INSERT INTO state VALUES (\"+String.fromCharCode(39)+\"retained\"+String.fromCharCode(39)+\")\");console.log(\"STATE_WRITTEN\")'";
function stream(response, tool){
 response.writeHead(200,{'content-type':'text/event-stream'});
 const emit=value=>response.write('data: '+JSON.stringify(value)+'\n\n');
 const item=tool ? {type:'function_call',id:'fc_state',call_id:'call_state',name:'exec_command',arguments:JSON.stringify({cmd:command})}
   : {type:'message',id:'msg_final',role:'assistant',content:[{type:'output_text',text:'CONTINUED_WITH_STATE',annotations:[]}]};
 emit({type:'response.created',response:{id:'resp_'+calls,object:'response',status:'in_progress',output:[]}});
 emit({type:'response.output_item.added',output_index:0,item});
 emit({type:'response.output_item.done',output_index:0,item});
 emit({type:'response.completed',response:{id:'resp_'+calls,object:'response',status:'completed',model:'gpt-5.3-codex',output:[item],usage:{input_tokens:10,output_tokens:10,total_tokens:20,input_tokens_details:{cached_tokens:0}}}});
 response.end();
}
const server=createServer(async(request,response)=>{
 const chunks=[];for await(const chunk of request)chunks.push(chunk);
 if(!request.url.startsWith('/v1/responses')){response.writeHead(404);response.end();return;}
 calls++;const body=JSON.parse(Buffer.concat(chunks).toString());firstRequest ??= body;
 if(!wrote){wrote=true;stream(response,true);return;}
 if(!allowed){response.writeHead(429,{'content-type':'application/json','retry-after':'0'});response.end(JSON.stringify({error:{type:'rate_limit_error',code:'rate_limit_exceeded',message:'fixture rate limit'}}));return;}
 resumedContext=JSON.stringify(body.input).includes('STATE_WRITTEN') && JSON.stringify(body.input).includes('Write the local application state.');
 stream(response,false);
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const env={...process.env,CODEX_HOME:'/home/developer/.codex',MODEL_PROXY_TOKEN:'fixture-not-a-real-key'};
function invoke(session,prompt){return new Promise((resolve,reject)=>{
 const args=['exec',...(session?['resume']:[]),'--json','--skip-git-repo-check','--dangerously-bypass-approvals-and-sandbox','--ignore-user-config','--ignore-rules','--model','gpt-5.3-codex',
 '-c','model_reasoning_effort="low"','-c','model_provider="model_proxy"','-c','web_search="disabled"','-c','features.multi_agent=false',
 '-c','model_providers.model_proxy.name="Model API"','-c','model_providers.model_proxy.base_url="http://127.0.0.1:'+server.address().port+'/v1"',
 '-c','model_providers.model_proxy.env_key="MODEL_PROXY_TOKEN"','-c','model_providers.model_proxy.wire_api="responses"',
 '-c','model_providers.model_proxy.request_max_retries=0','-c','model_providers.model_proxy.stream_max_retries=0',...(session?[session]:[]),'-'];
 const child=spawn(process.env.STACK_BENCH_TEST_CODEX || 'codex',args,{cwd:'/app',env,stdio:['pipe','pipe','pipe']});let out='',err='';
 child.stdout.on('data',b=>out+=b);child.stderr.on('data',b=>err+=b);
 const timer=setTimeout(()=>{child.kill('SIGKILL');reject(new Error('CLI timeout: '+err.slice(-800)));},45000);
 child.on('error',reject);child.on('close',code=>{clearTimeout(timer);try{resolve({code,events:out.trim().split('\n').map(line=>JSON.parse(line)),err});}catch{reject(new Error('CLI invalid output '+out.slice(-1000)+' '+err.slice(-1000)));}});child.stdin.end(prompt);
});}
function native(session){const visit=dir=>readdirSync(dir,{withFileTypes:true}).flatMap(e=>e.isDirectory()?visit(join(dir,e.name)):[join(dir,e.name)]);return visit(env.CODEX_HOME+'/sessions').find(path=>path.endsWith(session+'.jsonl'));}
function state(){const db=new DatabaseSync('/app/state.sqlite');try{return {file:readFileSync('/app/candidate.txt','utf8'),row:db.prepare('SELECT value FROM state').get().value};}finally{db.close();}}
try{
 const first=await invoke(null,'Write the local application state.');assert.notEqual(first.code,0,JSON.stringify(first));
 const sessionId=first.events.find(e=>e.type==='thread.started')?.thread_id;assert(sessionId,JSON.stringify(first));
 const path=native(sessionId),before=readFileSync(path,'utf8');let stateBefore;
 try{stateBefore=state();}catch(error){throw new Error(String(error)+' native: '+before.slice(-7000)+' events: '+JSON.stringify(first));}
 console.log(JSON.stringify({paused:true,sessionId,before,stateBefore,firstRequest,firstError:JSON.stringify(first.events)}));
 await new Promise(resolve=>process.stdin.once('data',resolve));allowed=true;
 const second=await invoke(sessionId,'Continue.');assert.equal(second.code,0,JSON.stringify(second));
 assert.equal(second.events.find(e=>e.type==='thread.started')?.thread_id,sessionId);
 assert(resumedContext,'native resume lost original task or tool result');
 const after=readFileSync(path,'utf8'),stateAfter=state();assert.deepEqual(stateAfter,stateBefore);
 console.log(JSON.stringify({sessionId,before,after,stateBefore,stateAfter,calls,firstError:JSON.stringify(first.events),second:second.events.find(e=>e.type==='item.completed'&&e.item.type==='agent_message')?.item.text}));
}finally{server.close();}
`;

for (const provider of ['anthropic', 'openai'] as const) test(`pinned ${provider} native resume retains settled tools and SQLite through campaign wait/control`, async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-native-provider-'));
  const script = join(root, 'fixture.mjs');
  const name = root.split(/[\\/]/).at(-1)!;
  const app = join(root, 'app');
  const adapter = CODING_PROVIDERS[provider];
  const native = adapter.projects(app);
  const model = provider === 'anthropic' ? 'claude-sonnet-5' : 'gpt-5.3-codex';
  assert(!existsSync(native));
  const image = process.env.STACK_BENCH_BUILD_IMAGE ?? DEFAULT_BUILD_IMAGE;
  const docker = (args: string[]) => {
    const result = spawnSync('docker', args, { encoding: 'utf8', timeout: 15_000 });
    assert.equal(result.status, 0, result.stderr); return result.stdout.trim();
  };
  try {
    mkdirSync(app); mkdirSync(native, { recursive: true });
    writeFileSync(script, provider === 'anthropic' ? fixture : codexFixture);
    const namespaceId = docker(['run', '-d', '--rm', '--name', `${name}-namespace`, '--network', 'none',
      '--entrypoint', 'sleep', image, '120']);
    const networkId = docker(['network', 'inspect', 'none', '--format', '{{.Id}}']);
    const directory = join(root, 'campaign');
    const plan = compileCampaignFile(join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'campaign.deterministic.json'));
    const initialized = initializeCampaignDirectory(plan, directory);
    const claimed = claimNextAttempt(initialized.state, { admissionId: 'native-test-admission' });
    assert(claimed.claim);
    writeCampaignState(initialized.paths.state, plan, claimed.state);
    const attemptId = claimed.claim.attempt.id;
    const execution = claimed.state.attempts.find(a => a.plan.id === attemptId)!.executions.at(-1)!;
    const marker = sha256('native-test');
    campaignLockTransaction({ operation: 'acquire', lock: { path: join(directory, '.campaign.lock.json'),
      token: 'native-test', record: { version: 2, campaignId: plan.id, campaignSha256: plan.contentSha256,
        ownerPid: process.pid, ownerInstance: controllerInstance(), ownershipMarkerSha256: marker,
        acquiredAt: new Date().toISOString() } } });
    const waitRoot = join(directory, execution.output, 'provider-waits');
    const waitEnv = { STACK_BENCH_PROVIDER_WAIT_CONTEXT: JSON.stringify({ directory,
      campaignSha256: plan.contentSha256, attemptId, executionId: execution.id,
      ownershipMarkerSha256: marker, root: waitRoot }) };
    const child = spawn('docker', ['run', '-i', '--rm', '--name', name, '--network', `container:${namespaceId}`,
      '--user', '10001:10001', '--cpus', '1', '--memory', '1g', '--memory-swap', '1g', '--pids-limit', '128',
      '--tmpfs', '/home/developer:rw,uid=10001,gid=10001',
      '--tmpfs', '/home/developer/.claude:rw,uid=10001,gid=10001',
      '--tmpfs', '/home/developer/.codex:rw,uid=10001,gid=10001',
      ...(provider === 'openai' && process.env.STACK_BENCH_TEST_CODEX_DIRECTORY ? [
        '--mount', `type=bind,source=${process.env.STACK_BENCH_TEST_CODEX_DIRECTORY},target=/codex-native,readonly`,
        '--env', 'STACK_BENCH_TEST_CODEX=/codex-native/bin/codex',
      ] : []),
      '--mount', `type=bind,source=${script},target=/fixture.mjs,readonly`,
      '--mount', `type=bind,source=${app},target=/app`,
      '--mount', `type=bind,source=${native},target=${adapter.containerTranscripts}`,
      '--entrypoint', 'node', image, '--experimental-sqlite', '/fixture.mjs'], { stdio: ['pipe', 'pipe', 'pipe'] });
    type Result = { sessionId: string; before: string; after: string; stateBefore: unknown;
      stateAfter: unknown; firstError: string; second: string; paused?: boolean; firstRequest?: unknown };
    const result = await new Promise<Result>((resolve, reject) => {
      let output = '', errors = ''; let final: Result | null = null;
      const timer = setTimeout(() => { child.kill(); reject(new Error('Native fixture timed out')); }, 100_000);
      child.stderr.on('data', chunk => { errors += String(chunk); });
      child.on('error', reject);
      child.stdout.on('data', chunk => {
        output += String(chunk);
        while (output.includes('\n')) {
          const index = output.indexOf('\n'); const line = output.slice(0, index); output = output.slice(index + 1);
          try {
            const event = JSON.parse(line) as Result;
            if (!event.paused) { final = event; continue; }
            if (provider === 'openai' && process.env.STACK_BENCH_TEST_NATIVE_CAPTURE) {
              writeFileSync(process.env.STACK_BENCH_TEST_NATIVE_CAPTURE, JSON.stringify({ request: event.firstRequest, native: event.before }));
            }
            try { adapter.validateContinuation(native, event.sessionId, model); }
            catch (error) {
              const shape = event.before.trim().split('\n').map(line => {
                const { type, uuid, parentUuid, sessionId } = JSON.parse(line) as Record<string, unknown>;
                return { type, uuid, parentUuid, sessionId };
              });
              throw new Error(`${String(error)}: ${JSON.stringify(shape)}`);
            }
            const containerId = docker(['inspect', name, '--format', '{{.Id}}']);
            const imageId = docker(['inspect', name, '--format', '{{.Image}}']);
            const lease = createBackendLease({ runId: name, backend: 'mongodb', track: 'ecommerce',
              runIndex: 91, database: 'app_ecom_run91', container: { name: `${name}-namespace`, id: namespaceId } });
            lease.state = 'active';
            lease.resources.buildContainer = { name, id: containerId, image: imageId, owned: true,
              networkMode: `container:${namespaceId}`, resourceLimits: { cpuCount: 1,
                memoryBytes: 1073741824, memorySwapBytes: 1073741824, pids: 128 } };
            lease.resources.network = { name: 'none', id: networkId, namespaceContainerId: namespaceId,
              hostAddresses: [], services: [], firewallSha256: null, firewallInstalledAt: null };
            const leasePath = join(root, 'lease.json'); writeBackendLease(leasePath, lease);
            const binding = { appDir: app, provider, sessionId: event.sessionId,
              model, imageId, env: { ...process.env,
                STACK_BENCH_LEASE: leasePath, STACK_BENCH_LEASE_TOKEN: lease.ownershipToken } };
            const identity = captureNativeContinuation(binding);
            const evidence = { actionId: 'native-action', invocation: 1, sessionId: event.sessionId,
              category: 'rate-limit', nativeIdentity: identity, fixture: true };
            persistCampaignProviderInvocation({ env: waitEnv, evidence });
            const accepted = waitForCampaignProviderContinuation({ env: waitEnv, evidence,
              validate: () => assert.equal(captureNativeContinuation(binding), identity),
              sleep: () => {
                assert.equal(readCampaignProviderContinuationStatus(directory, attemptId).eligible, true);
                requestCampaignProviderContinuation(directory, { attemptId, requestId: 'native-continue-1' });
              } });
            assert(accepted);
            assert.equal(JSON.parse(readFileSync(join(waitRoot, 'events', `${accepted.generation}.accepted.json`), 'utf8')).requestId, 'native-continue-1');
            child.stdin.end('continue\n');
          } catch (error) { clearTimeout(timer); child.kill(); reject(error); }
        }
      });
      child.on('close', code => { clearTimeout(timer);
        if (code !== 0 || !final) reject(new Error(`Native fixture exited ${code}: ${errors}`));
        else resolve(final);
      });
    });
    adapter.validateContinuation(native, result.sessionId, model);
    assert.deepEqual(result.stateAfter, result.stateBefore);
    assert.match(result.firstError, /429|rate limit/i);
    assert.equal(result.second, 'CONTINUED_WITH_STATE');
  } finally {
    spawnSync('docker', ['rm', '-f', name], { encoding: 'utf8', timeout: 10_000 });
    spawnSync('docker', ['rm', '-f', `${name}-namespace`], { encoding: 'utf8', timeout: 10_000 });
    rmSync(native, { recursive: true, force: true });
    rmSync(root, { recursive: true, force: true });
  }
});
