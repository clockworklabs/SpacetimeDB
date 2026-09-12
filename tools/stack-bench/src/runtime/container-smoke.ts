import { dockerMountArguments } from './container-mount.js';
import { dockerHostGatewayArguments, dockerHostServiceAddress, ATTEMPT_CREATION_LABEL,
  recordAttemptCreation, requireAttemptNetwork } from './docker-network.js';
import { updateBackendLease } from './backend-lease.js';
import type { BackendLease } from './backend-lease.js';
import { SIDECAR_CONTAINER_RESOURCE_LIMITS } from '../composition/product-config.js';

export interface ContainerSmokeResult {
  platform: string;
  arch: string;
  node: string;
  reached: Array<{ url: string; status: number }>;
  tcpReached: number[];
  executables: Record<string, string>;
  credentialStatus: string;
  diskFreeBytes: number;
}

export function runContainerSmoke({ command, imageId, resultsDir, destinations, tcpPorts,
  requiredExecutables, credentialStatusCommand, credentialMount, credentialEnvironment,
  marker, networkMode, leaseContext }: {
  command: (file: string, args: string[]) => string;
  imageId: string;
  resultsDir: string;
  destinations: string[];
  tcpPorts: number[];
  requiredExecutables: readonly string[];
  credentialStatusCommand: readonly string[] | null;
  credentialMount: { kind: string; source: string; target: string; readOnly: boolean } | null;
  credentialEnvironment: { name: string; file?: string } | null;
  marker: string;
  networkMode: string;
  leaseContext?: { path: string; lease: BackendLease };
}): ContainerSmokeResult {
  if (leaseContext) {
    networkMode = requireAttemptNetwork(leaseContext.lease);
    credentialMount = null;
    credentialEnvironment = null;
    credentialStatusCommand = null;
  }
  const hostAddress = dockerHostServiceAddress(networkMode);
  const script = `const fs=require('node:fs'),net=require('node:net'),path=require('node:path');`
    + `const {spawnSync}=require('node:child_process');`
    + `(async()=>{const urls=JSON.parse(process.argv[1]);const ports=JSON.parse(process.argv[2]);`
    + `const required=JSON.parse(process.argv[3]);const statusCommand=JSON.parse(process.argv[4]);`
    + `const credentialEnvironment=JSON.parse(process.argv[5]);`
    + `const tcpReached=[];let credentialStatus='not-checked';`
    + `const executablePaths=required.map(name=>{for(const dir of (process.env.PATH||'').split(':')){`
    + `const candidate=path.join(dir,name);try{fs.accessSync(candidate,fs.constants.X_OK);return candidate}catch{}}`
    + `throw new Error('required executable not found: '+name)});`
    + `if(credentialEnvironment){const value=credentialEnvironment.file`
    + `?fs.readFileSync(credentialEnvironment.file,'utf8').trim():process.env[credentialEnvironment.name];`
    + `if(!value)throw new Error('selected credential is empty');process.env[credentialEnvironment.name]=value}`
    + `if(statusCommand){const r=spawnSync(statusCommand[0],statusCommand.slice(1),{encoding:'utf8'});`
    + `credentialStatus=r.status===0?'ready':'not-ready'}`
    + `const reach=port=>new Promise((ok,fail)=>{const s=net.createConnection({host:'${hostAddress}',port});`
    + `const t=setTimeout(()=>s.destroy(new Error('timeout')),5000);s.once('connect',()=>{clearTimeout(t);s.end();ok()});`
    + `s.once('error',e=>{clearTimeout(t);fail(new Error('${hostAddress}:'+port+': '+e.message))})});`
    + `const reached=[];for(const url of urls){try{const r=await fetch(url,{method:'HEAD',signal:AbortSignal.timeout(15000)});`
    + `reached.push({url,status:r.status})}catch(e){throw new Error(url+': '+e.message)}}`
    + `for(const port of ports){await reach(port);tcpReached.push(port)}`
    + `fs.writeFileSync('/results/'+process.argv[6],'container-write-ok');const s=fs.statfsSync('/',{bigint:true});`
    + `process.stdout.write(JSON.stringify({platform:process.platform,arch:process.arch,node:process.version,reached,`
    + `tcpReached,executables:Object.fromEntries(required.map((name,index)=>[name,executablePaths[index]])),`
    + `credentialStatus,diskFreeBytes:Number(s.bavail*s.bsize)}))})()`;
  const intent = leaseContext ? recordAttemptCreation(leaseContext.path, leaseContext.lease, 'smoke') : null;
  const args = [intent ? 'create' : 'run', ...(intent ? ['--name', intent.name,
    '--label', `${ATTEMPT_CREATION_LABEL}=${intent.creationToken}`, '--cap-drop', 'ALL',
    '--security-opt', 'no-new-privileges:true', '--cpus', String(SIDECAR_CONTAINER_RESOURCE_LIMITS.cpuCount),
    '--memory', String(SIDECAR_CONTAINER_RESOURCE_LIMITS.memoryBytes),
    '--memory-swap', String(SIDECAR_CONTAINER_RESOURCE_LIMITS.memoryBytes),
    '--pids-limit', String(SIDECAR_CONTAINER_RESOURCE_LIMITS.pids)] : ['--rm']), '--network', networkMode,
    ...dockerHostGatewayArguments(networkMode),
    ...(credentialEnvironment && !credentialEnvironment.file ? ['-e', credentialEnvironment.name] : []),
    ...(credentialMount ? dockerMountArguments(credentialMount) : []),
    '-v', `${resultsDir}:/results`, '--entrypoint', 'node', imageId, '-e', script,
    JSON.stringify(destinations), JSON.stringify(tcpPorts), JSON.stringify(requiredExecutables),
    JSON.stringify(credentialStatusCommand), JSON.stringify(credentialEnvironment), marker];
  let id: string | null = null;
  let output: string;
  try {
    if (leaseContext && intent) {
      id = command('docker', args).trim();
      updateBackendLease(leaseContext.path, { token: leaseContext.lease.ownershipToken }, next => {
        next.resources.smokeContainer = { id: id!, name: intent.name, image: imageId,
          owned: true, networkMode };
        return next;
      });
      output = command('docker', ['start', '--attach', id]);
    } else output = command('docker', args);
  } finally {
    if (id && leaseContext) {
      command('docker', ['rm', '-f', '--volumes', id]);
      updateBackendLease(leaseContext.path, { token: leaseContext.lease.ownershipToken }, next => {
        delete next.resources.smokeContainer;
        delete next.resources.creationIntents?.smoke;
        return next;
      });
    }
  }
  return JSON.parse(output) as ContainerSmokeResult;
}
