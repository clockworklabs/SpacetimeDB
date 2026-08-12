#!/usr/bin/env node

import { execFileSync, spawnSync } from 'node:child_process';
import {
  existsSync, mkdirSync, openSync, closeSync, readFileSync, readSync, renameSync, rmSync,
  statfsSync, writeFileSync,
} from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { AGENT_ADAPTER_REGISTRY } from './agent-adapters.mjs';
import { parseImageId } from './container-image.mjs';
import { BUILD_OUTBOUND_DESTINATIONS, DEFAULT_BUILD_IMAGE,
  PREFLIGHT_RESOURCE_FLOORS } from './product-config.mjs';
import { resolveLegacyRecipeRelease } from './recipe-release.mjs';
import { resolveRecipeSelection } from './recipe-selection.mjs';
import { executeStackCapability } from './stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.mjs';
import { assertNoPortCollisions, listTracks, loadTrack, portsFor } from './tracks.mjs';
import { pidsOnPort } from './platform.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const COMPOSE = process.env.STACK_BENCH_COMPOSE_FILE ?? join(ROOT, 'docker-compose.yaml');
const LINUX_CLI = process.env.STACK_BENCH_LINUX_CLI
  ?? join(ROOT, 'container', 'bin', 'spacetimedb-cli');
const GIB = 1024 ** 3;

function splitList(value) {
  return String(value).split(',').map(item => item.trim()).filter(Boolean);
}

export function parsePreflightArgs(argv) {
  const request = { backends: [], track: 'ecommerce', levels: '1', runIndex: 0,
    agentAdapter: 'claude-code', packIds: [], checkKeys: [], smoke: false,
    image: process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE,
    resultsDir: join(ROOT, 'results') };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--backend': request.backends.push(...splitList(argv[++i])); break;
      case '--track': request.track = argv[++i]; break;
      case '--levels': request.levels = argv[++i]; break;
      case '--run-index': request.runIndex = Number(argv[++i]); break;
      case '--agent-adapter': request.agentAdapter = argv[++i]; break;
      case '--pack': request.packIds.push(...splitList(argv[++i])); break;
      case '--check': request.checkKeys.push(...splitList(argv[++i])); break;
      case '--image': request.image = argv[++i]; break;
      case '--results-dir': request.resultsDir = resolve(argv[++i]); break;
      case '--report': request.report = resolve(argv[++i]); break;
      case '--smoke': request.smoke = true; break;
      case '--json': request.json = true; break;
      default: throw new Error(`unknown argument: ${argv[i]}`);
    }
  }
  if (!request.backends.length) throw new Error('--backend is required (comma-separated values are accepted)');
  request.backends = [...new Set(request.backends)].sort();
  if (!Number.isInteger(request.runIndex) || request.runIndex < 0) throw new Error('--run-index must be a non-negative integer');
  const match = String(request.levels).match(/^(\d+)(?:-(\d+))?$/);
  if (!match || Number(match[2] ?? match[1]) < Number(match[1])) throw new Error('--levels must be N or N-M');
  request.levelList = Array.from({ length: Number(match[2] ?? match[1]) - Number(match[1]) + 1 },
    (_, index) => Number(match[1]) + index);
  return request;
}

function commandRunner(run) {
  return (file, args) => String(run(file, args, {
    encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout: 120_000,
  })).trim();
}

function bytes(value) {
  return `${(Number(value) / GIB).toFixed(1)} GiB`;
}

function composeImageReference(service, text) {
  const section = text.match(new RegExp(`^  ${service}:\\r?\\n([\\s\\S]*?)(?=^  [a-z][a-z0-9-]*:|^volumes:)`, 'm'))?.[1] ?? '';
  return section.match(/^    image:\s*(\S+)\s*$/m)?.[1] ?? null;
}

function checkResult(id, status, summary, remediation = null, evidence = null) {
  return { id, status, summary, ...(remediation ? { remediation } : {}),
    ...(evidence ? { evidence } : {}) };
}

function credentialReady(adapter, env, home, exists) {
  const environment = adapter.apiKeyEnvironmentVariable;
  if (environment && env[environment]) return { ok: true, source: `environment:${environment}` };
  const keyFileVariable = environment ? `${environment}_FILE` : null;
  if (keyFileVariable && env[keyFileVariable] && exists(resolve(env[keyFileVariable]))) {
    return { ok: true, source: `secret-file:${keyFileVariable}` };
  }
  const file = adapter.credentialFiles.find(relative => exists(join(home, relative)));
  if (file) return { ok: true, source: `file:${file.replaceAll('\\', '/')}` };
  if (!environment && adapter.credentialFiles.length === 0) return { ok: true, source: 'not-required' };
  return { ok: false, source: null, environment, files: adapter.credentialFiles };
}

function inspectLinuxCli(path = LINUX_CLI) {
  if (!existsSync(path)) return { ok: false, arch: null };
  const header = Buffer.alloc(20);
  try {
    const descriptor = openSync(path, 'r');
    try { readSync(descriptor, header, 0, header.length, 0); } finally { closeSync(descriptor); }
    if (!header.subarray(0, 4).equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46]))) {
      return { ok: false, arch: null };
    }
    const machine = header.readUInt16LE(18);
    return { ok: [0x3e, 0xb7].includes(machine), arch: machine === 0x3e ? 'x86_64'
      : machine === 0xb7 ? 'aarch64' : `machine-${machine}` };
  } catch { return { ok: false, arch: null }; }
}

export function probeLoopbackPort(port, { spawn = spawnSync } = {}) {
  if (!Number.isInteger(Number(port)) || Number(port) < 1 || Number(port) > 65535) {
    throw new Error(`invalid TCP port ${port}`);
  }
  const script = "const net=require('node:net');const s=net.createServer();"
    + "s.once('error',e=>{console.error(e.code||e.message);process.exit(e.code==='EADDRINUSE'?3:2)});"
    + "s.listen(Number(process.argv[1]),'127.0.0.1',()=>s.close(()=>process.exit(0)));";
  const result = spawn(process.execPath, ['-e', script, String(port)], {
    encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout: 10_000,
  });
  if (result.status === 0) return { free: true };
  if (result.status === 3 && String(result.stderr).includes('EADDRINUSE')) return { free: false };
  throw new Error(String(result.stderr || result.error?.message || `probe exited ${result.status}`).trim());
}

function runSmoke({ command, imageId, resultsDir, destinations, marker }) {
  const script = `const fs=require('node:fs');(async()=>{const urls=JSON.parse(process.argv[1]);`
    + `const reached=[];for(const url of urls){try{const r=await fetch(url,{method:'HEAD',signal:AbortSignal.timeout(15000)});`
    + `reached.push({url,status:r.status})}catch(e){throw new Error(url+': '+e.message)}}`
    + `fs.writeFileSync('/results/'+process.argv[2],'container-write-ok');const s=fs.statfsSync('/',{bigint:true});`
    + `process.stdout.write(JSON.stringify({platform:process.platform,arch:process.arch,node:process.version,reached,`
    + `diskFreeBytes:Number(s.bavail*s.bsize)}))})()`;
  const output = command('docker', ['run', '--rm', '--add-host', 'host.docker.internal:host-gateway',
    '-v', `${resultsDir}:/results`, imageId, 'node', '-e', script,
    JSON.stringify(destinations), marker]);
  return JSON.parse(output);
}

export function runPreflight(request, dependencies = {}) {
  const run = commandRunner(dependencies.run ?? execFileSync);
  const env = dependencies.env ?? process.env;
  const now = dependencies.now ?? Date.now();
  const home = dependencies.home ?? homedir();
  const exists = dependencies.exists ?? existsSync;
  const inspectPorts = dependencies.pidsOnPort ?? pidsOnPort;
  const probePort = dependencies.probePort ?? probeLoopbackPort;
  const checks = [];
  const add = (...args) => checks.push(checkResult(...args));
  let track;
  let agent;

  try {
    agent = AGENT_ADAPTER_REGISTRY.get(request.agentAdapter);
    for (const backend of request.backends) STACK_ADAPTER_REGISTRY.get(backend);
    if (!listTracks({ includeInternal: true }).includes(request.track)) {
      throw new Error(`unknown track ${JSON.stringify(request.track)}`);
    }
    track = loadTrack(request.track);
    assertNoPortCollisions();
    for (const level of request.levelList) {
      const binding = resolveLegacyRecipeRelease(track, level);
      if (!binding && (request.packIds.length || request.checkKeys.length)) {
        throw new Error(`L${level} has no recipe release for --pack/--check selection`);
      }
      if (binding) resolveRecipeSelection(binding.release, request);
    }
    add('request.scope', 'pass', `${request.track} L${request.levelList.join(',L')} on ${request.backends.join(', ')}`);
  } catch (error) {
    add('request.scope', 'fail', String(error.message), 'Correct the requested track, level, pack, check, or adapter.');
  }

  const nodeMajor = Number(process.versions.node.split('.')[0]);
  add('host.node', nodeMajor >= 22 ? 'pass' : 'fail', `Node ${process.versions.node}`,
    nodeMajor >= 22 ? null : 'Install Node 22 or newer.');
  const appliance = env.STACK_BENCH_APPLIANCE === '1';
  const supportedHost = appliance
    ? process.platform === 'linux' && process.arch === 'x64'
    : ['win32', 'linux'].includes(process.platform) && ['x64', 'arm64'].includes(process.arch);
  add('host.architecture', supportedHost ? 'pass' : 'fail', `${process.platform}/${process.arch}`,
    supportedHost ? null : appliance ? 'Run the appliance on its supported linux/amd64 dedicated runner.'
      : 'Use a supported x64 or arm64 Windows/Linux host.');

  for (const name of ['STACK_BENCH_LEASE', 'STACK_BENCH_LEASE_TOKEN', 'STACK_BENCH_SUPERVISOR_STATE']) {
    if (env[name]) add(`ambient.${name.toLowerCase()}`, 'fail', `${name} is already set`,
      `Unset ${name}; preflight must not inherit another run's ownership state.`);
  }
  if (env.DOCKER_HOST && /^(?:tcp|ssh):/i.test(env.DOCKER_HOST)) {
    add('ambient.docker-host', 'fail', 'Remote Docker endpoint is configured',
      'Use a local Docker engine; bind mounts and host routing are part of the measured environment.');
  } else add('ambient.docker-host', 'pass', 'Docker endpoint is local/default');

  const auth = agent ? credentialReady(agent, env, home, exists) : { ok: false };
  add('agent.credentials', auth.ok ? 'pass' : 'fail', auth.ok
    ? `Credential source available (${auth.source})` : 'No declared credential source is available',
  auth.ok ? null : `Set ${auth.environment ?? 'the adapter API key'} or install one of: ${(auth.files ?? []).join(', ')}`);

  mkdirSync(request.resultsDir, { recursive: true });
  const hostMarker = `.preflight-host-${process.pid}-${Math.random().toString(16).slice(2)}`;
  try {
    writeFileSync(join(request.resultsDir, hostMarker), 'host-write-ok', { flag: 'wx', mode: 0o600 });
    rmSync(join(request.resultsDir, hostMarker));
    const stats = statfsSync(request.resultsDir, { bigint: true });
    const free = stats.bavail * stats.bsize;
    add('storage.results', free >= BigInt(PREFLIGHT_RESOURCE_FLOORS.resultDiskBytes) ? 'pass' : 'fail',
      `${bytes(free)} free at ${request.resultsDir}`,
      free >= BigInt(PREFLIGHT_RESOURCE_FLOORS.resultDiskBytes) ? null
        : `Provide at least ${bytes(PREFLIGHT_RESOURCE_FLOORS.resultDiskBytes)} free for persistent results.`);
  } catch (error) {
    add('storage.results', 'fail', `Result directory is not safely writable: ${error.message}`,
      'Choose a persistent writable result directory with --results-dir.');
  }

  let dockerInfo = null;
  let imageId = null;
  try {
    dockerInfo = JSON.parse(run('docker', ['info', '--format', '{{json .}}']));
    const ok = dockerInfo.OSType === 'linux' && (appliance
      ? dockerInfo.Architecture === 'x86_64'
      : ['x86_64', 'aarch64'].includes(dockerInfo.Architecture));
    add('docker.engine', ok ? 'pass' : 'fail',
      `Docker ${dockerInfo.ServerVersion ?? 'unknown'} (${dockerInfo.OSType}/${dockerInfo.Architecture})`,
      ok ? null : 'Use a supported x86_64 or aarch64 Linux-container Docker engine.');
    try {
      add('docker.compose', 'pass', `Docker Compose ${run('docker', ['compose', 'version', '--short'])}`);
    } catch {
      add('docker.compose', 'fail', 'Docker Compose plugin is unavailable',
        'Install the Docker Compose v2 plugin.');
    }
    const enoughCpu = dockerInfo.NCPU >= PREFLIGHT_RESOURCE_FLOORS.cpuCount;
    add('docker.cpu', enoughCpu ? 'pass' : 'fail', `${dockerInfo.NCPU} CPUs available`,
      enoughCpu ? null : `Allocate at least ${PREFLIGHT_RESOURCE_FLOORS.cpuCount} CPUs to Docker.`);
    const enoughMemory = dockerInfo.MemTotal >= PREFLIGHT_RESOURCE_FLOORS.memoryBytes;
    add('docker.memory', enoughMemory ? 'pass' : 'fail',
      `${bytes(dockerInfo.MemTotal)} available`,
      enoughMemory ? null : `Allocate at least ${bytes(PREFLIGHT_RESOURCE_FLOORS.memoryBytes)} to Docker.`);
    const skew = Math.abs(Date.parse(dockerInfo.SystemTime) - now);
    const clockReady = Number.isFinite(skew) && skew <= PREFLIGHT_RESOURCE_FLOORS.clockSkewMs;
    add('docker.clock', clockReady ? 'pass' : 'fail',
      `host/engine skew ${Number.isFinite(skew) ? skew : 'unknown'} ms`,
      clockReady ? null : 'Synchronize the host and Docker clocks.');
  } catch (error) {
    add('docker.engine', 'fail', `Docker is unavailable: ${String(error.message).split('\n')[0]}`,
      'Start a local Docker engine and ensure the docker CLI can reach it.');
  }

  if (dockerInfo) {
    try {
      imageId = parseImageId(run('docker', ['image', 'inspect', '--format', '{{.Id}}', request.image]));
      add('image.build', 'pass', `${request.image} -> ${imageId}`);
      const imagePlatform = run('docker', ['image', 'inspect', '--format', '{{.Os}}/{{.Architecture}}', request.image]);
      const expectedArch = dockerInfo.Architecture === 'x86_64' ? 'amd64' : 'arm64';
      add('image.platform', imagePlatform === `linux/${expectedArch}` ? 'pass' : 'fail', imagePlatform,
        imagePlatform === `linux/${expectedArch}` ? null : 'Build or pull the isolation image for the Docker engine architecture.');
    } catch (error) {
      add('image.build', 'fail', `Build image ${request.image} is unavailable or unidentifiable`,
        `Build it with: docker build -t ${request.image} ${join(ROOT, 'container')}`);
    }
  }

  if (!request.smoke) add('outbound.container', 'warn',
    'Container outbound access and result-volume persistence were not exercised',
    'Run the same command with --smoke before a paid campaign.');

  const composeText = exists(COMPOSE) ? String(dependencies.readCompose?.() ?? readFileSync(COMPOSE, 'utf8')) : '';
  for (const backend of (track ? request.backends : []).filter(item => ['postgres', 'mongodb'].includes(item))) {
    const service = backend === 'postgres' ? 'postgres' : 'mongodb';
    const containerName = `stack-bench-${backend}`;
    const reference = composeImageReference(service, composeText);
    if (!reference || !/@sha256:[a-f0-9]{64}$/.test(reference)) {
      add(`image.${backend}`, 'fail', `${service} Compose image is not digest-pinned`,
        'Pin the service image to an exact sha256 digest.');
      continue;
    }
    try {
      const expectedId = parseImageId(run('docker', ['image', 'inspect', '--format', '{{.Id}}', reference]));
      const inspected = JSON.parse(run('docker', ['container', 'inspect', containerName]))[0];
      const allocated = portsFor(track, backend, request.runIndex);
      const hostPort = String(allocated.dbPort ?? allocated.db);
      const mapping = inspected.NetworkSettings?.Ports?.[`${backend === 'postgres' ? 5432 : 27017}/tcp`] ?? [];
      const healthy = !inspected.State?.Health || inspected.State.Health.Status === 'healthy';
      const ready = inspected.State?.Running === true && healthy && inspected.Image === expectedId
        && mapping.some(item => item.HostPort === hostPort);
      add(`service.${backend}`, ready ? 'pass' : 'fail', ready
        ? `${containerName} is running exact image ${expectedId}`
        : `${containerName} is absent, stopped/unhealthy, on the wrong image, or mapped to the wrong port`,
      ready ? null : `Run docker compose -f ${COMPOSE} up -d ${service}.`);
    } catch (error) {
      add(`service.${backend}`, 'fail', `${containerName} is not ready`,
        `Run docker compose -f ${COMPOSE} up -d ${service}.`);
    }
  }

  if (track) {
    for (const backend of request.backends) {
      const ports = portsFor(track, backend, request.runIndex);
      for (const [role, port] of Object.entries(ports)) {
        if (port == null || role === 'db' || role === 'dbPort') continue;
        try {
          const availability = probePort(port);
          const listeners = availability.free ? [] : inspectPorts(port);
          add(`port.${backend}.${role}`, availability.free ? 'pass' : 'fail', availability.free
            ? `Port ${port} is free`
            : `Port ${port} is already in use${listeners.length ? ` by PID(s) ${listeners.join(', ')}` : ''}`,
          availability.free ? null : 'Stop the listener or choose a different --run-index.');
        } catch (error) {
          add(`port.${backend}.${role}`, 'fail', `Cannot prove port ${port} is free`,
            'Install netstat on Windows or lsof/ss on Linux so port ownership can be checked.');
        }
      }
    }
  }

  if (request.backends.includes('spacetime')) {
    try {
      const runtime = executeStackCapability(STACK_ADAPTER_REGISTRY.get('spacetime'),
        'orchestrator', 'config', { root: ROOT, env, helpers: { exists } });
      const port = Number(new URL(runtime.lease.serverUri).port);
      const availability = probePort(port);
      const listeners = availability.free ? [] : inspectPorts(port);
      add('port.spacetime.host', availability.free ? 'pass' : 'fail', availability.free
        ? `Dedicated host port ${port} is free`
        : `Dedicated host port ${port} is already in use${listeners.length ? ` by PID(s) ${listeners.join(', ')}` : ''}`,
      availability.free ? null : 'Stop the listener or choose another loopback STACK_BENCH_STDB_URI port.');
    } catch (error) {
      add('port.spacetime.host', 'fail', `Cannot validate the dedicated SpacetimeDB host port: ${error.message}`,
        'Use an explicit, free loopback STACK_BENCH_STDB_URI port.');
    }
    const cli = inspectLinuxCli();
    const expectedCliArch = dockerInfo?.Architecture ?? null;
    const cliReady = cli.ok && (!expectedCliArch || cli.arch === expectedCliArch);
    add('spacetime.linux-cli', cliReady ? 'pass' : 'fail', cliReady
      ? `Repository Linux CLI is ELF ${cli.arch}`
      : `Repository Linux CLI is missing, invalid, or ${cli.arch ?? 'unknown'} instead of ${expectedCliArch ?? 'the target architecture'}`,
    cliReady ? null : 'Run bash tools/stack-bench/container/build-linux-cli.sh on the target architecture.');
  }

  if (request.smoke && imageId) {
    const marker = `.preflight-container-${process.pid}-${Math.random().toString(16).slice(2)}`;
    const destinations = [...new Set([...BUILD_OUTBOUND_DESTINATIONS,
      ...(agent?.outboundDestinations ?? [])])].sort();
    try {
      const smoke = runSmoke({ command: run, imageId, resultsDir: request.resultsDir,
        destinations, marker });
      const persisted = exists(join(request.resultsDir, marker));
      rmSync(join(request.resultsDir, marker), { force: true });
      const smokeReady = smoke.platform === 'linux' && Number(smoke.node?.match(/^v(\d+)/)?.[1]) >= 22
        && persisted;
      add('smoke.container', smokeReady ? 'pass' : 'fail',
        `Container ${smoke.platform}/${smoke.arch} ${smoke.node}; ${smoke.reached.length} outbound destination(s); result mount ${persisted ? 'persistent' : 'failed'}`,
      smokeReady ? null : 'Fix container Node/runtime, networking, or the persistent results mount.', smoke);
      add('storage.container', smoke.diskFreeBytes >= PREFLIGHT_RESOURCE_FLOORS.resultDiskBytes ? 'pass' : 'fail',
        `${bytes(smoke.diskFreeBytes)} free in Docker storage`,
        smoke.diskFreeBytes >= PREFLIGHT_RESOURCE_FLOORS.resultDiskBytes ? null
          : `Provide at least ${bytes(PREFLIGHT_RESOURCE_FLOORS.resultDiskBytes)} free in Docker storage.`);
    } catch (error) {
      rmSync(join(request.resultsDir, marker), { force: true });
      add('smoke.container', 'fail', `No-model container smoke failed: ${String(error.message).split('\n')[0]}`,
        'Fix build-image startup, outbound TLS/DNS, host routing, or the result-volume mount.');
    }
  }

  const report = {
    schemaVersion: 1,
    generatedAt: new Date(now).toISOString(),
    request: { backends: request.backends, track: request.track, levels: request.levelList,
      runIndex: request.runIndex, agentAdapter: request.agentAdapter, packs: request.packIds,
      checks: request.checkKeys, image: request.image, resultsDir: request.resultsDir,
      smoke: request.smoke },
    ok: !checks.some(check => check.status === 'fail'),
    summary: { passed: checks.filter(check => check.status === 'pass').length,
      failed: checks.filter(check => check.status === 'fail').length,
      warnings: checks.filter(check => check.status === 'warn').length },
    checks,
  };
  return report;
}

function writeReport(path, report) {
  mkdirSync(dirname(path), { recursive: true });
  const temporary = `${path}.tmp-${process.pid}`;
  writeFileSync(temporary, `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx', mode: 0o600 });
  renameSync(temporary, path);
}

function printHuman(report) {
  console.log(`Stack Bench preflight: ${report.ok ? 'READY' : 'NOT READY'}`);
  for (const check of report.checks) {
    const mark = check.status === 'pass' ? 'PASS' : check.status === 'warn' ? 'WARN' : 'FAIL';
    console.log(`  ${mark.padEnd(4)}  ${check.id.padEnd(28)} ${check.summary}`);
    if (check.remediation && check.status === 'fail') console.log(`        fix: ${check.remediation}`);
  }
  console.log(`\n${report.summary.passed} passed, ${report.summary.failed} failed, ${report.summary.warnings} warnings`);
}

async function main() {
  let request;
  try { request = parsePreflightArgs(process.argv); }
  catch (error) {
    console.error(`preflight: ${error.message}`);
    console.error('Usage: node preflight.mjs --backend spacetime[,postgres,mongodb] [--track ecommerce] [--levels 1-2] [--smoke]');
    process.exit(2);
  }
  const report = runPreflight(request);
  if (request.report) writeReport(request.report, report);
  if (request.json) console.log(JSON.stringify(report, null, 2));
  else printHuman(report);
  process.exitCode = report.ok ? 0 : 1;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
