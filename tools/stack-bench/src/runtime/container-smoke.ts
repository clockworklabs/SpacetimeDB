import { dockerMountArguments } from './container-mount.js';
import { dockerHostGatewayArguments, dockerHostServiceAddress } from './docker-network.js';

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
  marker, networkMode }: {
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
  networkMode: 'host' | 'bridge';
}): ContainerSmokeResult {
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
  const output = command('docker', ['run', '--rm', '--network', networkMode,
    ...dockerHostGatewayArguments(networkMode),
    ...(credentialEnvironment && !credentialEnvironment.file ? ['-e', credentialEnvironment.name] : []),
    ...(credentialMount ? dockerMountArguments(credentialMount) : []),
    '-v', `${resultsDir}:/results`, imageId, 'node', '-e', script,
    JSON.stringify(destinations), JSON.stringify(tcpPorts), JSON.stringify(requiredExecutables),
    JSON.stringify(credentialStatusCommand), JSON.stringify(credentialEnvironment), marker]);
  return JSON.parse(output) as ContainerSmokeResult;
}
