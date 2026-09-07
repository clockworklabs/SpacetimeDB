import { execFileSync } from 'node:child_process';
import { chmodSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { resolveContainerImage } from '../src/runtime/container-image.js';
import { DATABASE_IMAGES } from '../src/stacks/database-containers.js';

export const STATE_VOLUME = 'stack-bench-state';
type Docker = (args: readonly string[]) => string;
const docker: Docker = args => execFileSync('docker', [...args], {
  encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout: args[0] === 'pull' ? 300_000 : 60_000,
});

/** The controller and Docker daemon see one native Linux path on every host OS. */
export function prepareStateVolume(env: NodeJS.ProcessEnv = process.env, run: Docker = docker): string {
  if (run(['version', '--format', '{{.Server.Os}}']).trim() !== 'linux') {
    throw new Error('Stack Bench requires a Docker daemon running Linux containers');
  }
  const inspect = (_command: string, args: readonly string[]) => run(args);
  const controller = resolveContainerImage(env.STACK_BENCH_CONTROLLER_IMAGE
    ?? 'stack-bench-controller:local', inspect).id;
  const build = resolveContainerImage(env.STACK_BENCH_BUILD_IMAGE
    ?? 'stack-bench-build:local', inspect).id;
  for (const reference of Object.values(DATABASE_IMAGES)) {
    try { resolveContainerImage(reference, inspect); }
    catch {
      run(['pull', '--platform', 'linux/amd64', reference]);
      resolveContainerImage(reference, inspect);
    }
  }
  run(['volume', 'create', STATE_VOLUME]);
  const root = run(['volume', 'inspect', '--format', '{{.Mountpoint}}', STATE_VOLUME]).trim();
  if (!/^\/[A-Za-z0-9_./-]+$/.test(root) || root.split('/').includes('..')) {
    throw new Error('Docker state volume has an invalid Linux mountpoint');
  }
  run(['run', '--rm', '--platform', 'linux/amd64', '--network', 'none', '--mount',
    `type=volume,source=${STATE_VOLUME},target=${root}`, '--entrypoint', 'node', controller, '-e',
    'const fs=require("node:fs"),p=require("node:path"),crypto=require("node:crypto");'
      + 'const root=process.argv[1];'
      + 'for(const name of ["work","results/plans","secrets","controller-home"])'
      + 'fs.mkdirSync(p.join(root,name),{recursive:true,mode:0o700});'
      + 'const secret=p.join(root,"secrets/dashboard_control_secret");'
      + 'if(!fs.existsSync(secret))fs.writeFileSync(secret,crypto.randomBytes(32).toString("hex")+"\\n",{flag:"wx",mode:0o600});'
      + 'for(const [source,name] of [["campaign.example.json","reference-check.json"],["campaign.ecommerce-progression-reference.json","ecommerce-progression.json"],["campaign.demo.json","demo.json"]]) {'
      + 'const target=p.join(root,"results/plans",name);if(!fs.existsSync(target))'
      + 'fs.copyFileSync(p.join("/opt/stack-bench/appliance",source),target,fs.constants.COPYFILE_EXCL);}'
      + 'const demo=p.join(root,"results/plans/paid-l1.json");if(!fs.existsSync(demo)){'
      + 'const plan=JSON.parse(fs.readFileSync("/opt/stack-bench/appliance/campaign.paid-l1.json","utf8"));'
      + 'plan.runtime.controllerImage=process.argv[2];plan.runtime.buildImage=process.argv[3];plan.state="frozen";'
      + 'fs.writeFileSync(demo,JSON.stringify(plan,null,2)+"\\n",{flag:"wx",mode:0o600});}',
    root, controller, build]);
  return [
    `STACK_BENCH_STATE_ROOT=${root}`,
    `STACK_BENCH_CONTROLLER_IMAGE=${controller}`,
    `STACK_BENCH_BUILD_IMAGE=${build}`,
    'STACK_BENCH_RUNNER_CAPACITY=1',
    'STACK_BENCH_AGENT_AUTH=subscription-token',
    `STACK_BENCH_CLAUDE_OAUTH_TOKEN_FILE=${root}/secrets/claude_subscription_token`,
    `STACK_BENCH_ANTHROPIC_API_KEY_FILE=${root}/secrets/anthropic_api_key`,
    `STACK_BENCH_DASHBOARD_CONTROL_SECRET_FILE=${root}/secrets/dashboard_control_secret`,
    'STACK_BENCH_RELEASE_MANIFEST=',
    '',
  ].join('\n');
}

export function writeStateSecret(name: string | undefined, input: string,
  root = '/state'): void {
  if (!['claude_subscription_token', 'anthropic_api_key', 'dashboard_control_secret'].includes(name ?? '')) {
    throw new Error('secret name must be claude_subscription_token, anthropic_api_key, or dashboard_control_secret');
  }
  const value = input.trim();
  if (!value || /[\r\n]/.test(value)
    || (name === 'dashboard_control_secret' && value.length < 32)) {
    throw new Error('secret must be one non-empty line; dashboard control secrets need at least 32 characters');
  }
  mkdirSync(join(root, 'secrets'), { recursive: true, mode: 0o700 });
  chmodSync(join(root, 'secrets'), 0o700);
  writeFileSync(join(root, 'secrets', name!), `${value}\n`, { mode: 0o600 });
  chmodSync(join(root, 'secrets', name!), 0o600);
}

export function stateVolumeCommand(command: string, args: string[]): void {
  if (command === 'setup') {
    if (args.length) throw new Error('setup accepts no arguments; configure image references through the environment');
    process.stdout.write(prepareStateVolume());
  } else {
    if (args.length !== 1) throw new Error('set-secret requires exactly one secret name');
    writeStateSecret(args[0], readFileSync(0, 'utf8'));
  }
}
