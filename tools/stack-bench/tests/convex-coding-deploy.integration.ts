import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { runBuild } from '../container/run-build.js';
import { workRoot } from '../src/composition/tracks.js';
import { backendResourceLockKeys, claimBackendResources, createBackendLease, publicBackendLease,
  readBackendLease, resourceLockScope } from '../src/runtime/backend-lease.js';
import { codingContainerAgentCommand, codingContainerAgentExecOptions } from '../src/runtime/coding-container-policy.js';
import { attemptDocker, requireAttemptNetwork } from '../src/runtime/docker-network.js';
import { standardBuildContainerPlan } from '../src/stacks/stack-agent-operations.js';
import { activateConvex, releaseConvex } from '../src/stacks/backends/convex-lifecycle.js';

test('trusted build plan cannot start a paid session or enable Convex in the CLI', async () => {
  await assert.rejects(runBuild([], standardBuildContainerPlan()), /requires --prepare-only/);
  const cli = spawnSync(process.execPath, [fileURLToPath(new URL('../container/run-build.js', import.meta.url)),
    '--prepare-only', '--app', 'unused', '--backend', 'convex'], { encoding: 'utf8' });
  assert.equal(cli.status, 2);
  assert.match(cli.stderr, /unknown stack adapter/i);
});

test('private Convex fixture deploys as the normal coding user with owned credentials', {
  skip: process.env.STACK_BENCH_CONVEX_OWNED_TEST !== '1', timeout: 180_000,
}, async () => {
  assert.equal(process.platform, 'linux');
  assert(process.env.STACK_BENCH_WORK_DIR, 'Require the normal daemon-visible work directory');
  const fixture = process.env.STACK_BENCH_CONVEX_FIXTURE!;
  const evidenceDirectory = process.env.STACK_BENCH_CONVEX_EVIDENCE_DIR!;
  const image = process.env.STACK_BENCH_BUILD_IMAGE!;
  assert(fixture && evidenceDirectory && /^sha256:[a-f0-9]{64}$/.test(image));
  mkdirSync(workRoot(), { recursive: true });
  const root = mkdtempSync(join(workRoot(), 'convex-coding-deploy-'));
  const app = join(root, 'app');
  const leasePath = join(root, 'lease.json');
  const ports = { vite: 14315, express: 14317, dbPort: null };
  const lease = createBackendLease({ runId: root.split('/').at(-1)!, backend: 'convex',
    track: 'ecommerce', runIndex: 2, serverUri: 'http://127.0.0.1:14316' });
  const evidence: Record<string, unknown> = { result: 'running', image,
    note: 'Pinned app source deployed by coding UID 10001; no model call or adapter registration.' };
  const save = () => writeFileSync(join(evidenceDirectory, 'coding-deploy.json'), JSON.stringify(evidence, null, 2));
  const buildUrl = new URL('../container/run-build.js', import.meta.url).href;
  const planUrl = new URL('../src/stacks/stack-agent-operations.js', import.meta.url).href;
  let active;
  let volumes: string[] = [];
  let failure: unknown;
  save();
  try {
    claimBackendResources(leasePath, lease, { ...resourceLockScope(), keys: backendResourceLockKeys(lease, ports) });
    activateConvex({ leasePath, leaseToken: lease.ownershipToken, ports });
    active = readBackendLease(leasePath, { active: true });
    const backend = active.resources.container!;
    volumes = JSON.parse(attemptDocker(['inspect', '--format', '{{json .Mounts}}', backend.id]))
      .filter((mount: { Type: string }) => mount.Type === 'volume').map((mount: { Name: string }) => mount.Name);
    mkdirSync(app);
    // Only application source and its setup enter the coding workspace. No probes or readers.
    for (const name of ['package.json', 'package-lock.json', 'convex.json', 'prepare-auth.mjs', 'convex']) {
      cpSync(join(fixture, name), join(app, name), { recursive: true });
    }
    const sourceFiles = ['package.json', 'package-lock.json', 'convex.json', 'prepare-auth.mjs',
      ...readdirSync(join(app, 'convex')).filter(name => name.endsWith('.js')).map(name => `convex/${name}`)].sort();
    const sourceHashes = () => Object.fromEntries(sourceFiles.map(name =>
      [name, createHash('sha256').update(readFileSync(join(app, name))).digest('hex')]));
    evidence.submittedSource = sourceHashes();
    const prepare = () => execFileSync(process.execPath, ['--input-type=module', '-e',
      `import {runBuild} from ${JSON.stringify(buildUrl)}; import {standardBuildContainerPlan} from ${JSON.stringify(planUrl)}; `
      + 'await runBuild(JSON.parse(process.env.CONVEX_PREPARE_ARGS),standardBuildContainerPlan());'], {
      env: { ...process.env, STACK_BENCH_LEASE: leasePath, STACK_BENCH_LEASE_TOKEN: lease.ownershipToken,
        CONVEX_PREPARE_ARGS: JSON.stringify(['--prepare-only', '--backend', 'convex', '--app', app, '--image', image]) },
      encoding: 'utf8', stdio: 'pipe', timeout: 90_000,
    });
    evidence.prepared = JSON.parse(prepare().trim());
    active = readBackendLease(leasePath, { active: true });
    const coding = active.resources.buildContainer!;
    const inspected = JSON.parse(attemptDocker(['inspect', coding.id]))[0];
    assert.equal(inspected.Image, image);
    assert.equal(inspected.HostConfig.NetworkMode, requireAttemptNetwork(active));
    assert.equal(inspected.HostConfig.ReadonlyRootfs, true);
    assert.deepEqual(inspected.Mounts.filter((mount: { Type: string }) => mount.Type === 'bind')
      .map((mount: { Source: string; Destination: string }) => [mount.Source, mount.Destination]), [[app, '/app']]);
    assert(!inspected.Config.Env.some((value: string) => /^(?:STACK_BENCH_LEASE|CONVEX_SELF_HOSTED_ADMIN_KEY|OPENAI_API_KEY|ANTHROPIC_API_KEY)=/.test(value)));
    const settings = { CONVEX_SELF_HOSTED_URL: active.resources.serverUri!,
      CONVEX_SELF_HOSTED_ADMIN_KEY: attemptDocker(['exec', backend.id, 'bash', './generate_admin_key.sh']) };
    const developer = (command: string, args: string[]) => execFileSync('docker', ['exec',
      ...codingContainerAgentExecOptions(), '-w', '/app',
      ...Object.keys(settings).flatMap(name => ['-e', name]), coding.id, ...codingContainerAgentCommand(command, args)], {
      env: { ...process.env, ...settings }, encoding: 'utf8', stdio: 'pipe', timeout: 90_000,
    });
    assert.equal(developer('id', ['-u']).trim(), '10001');
    developer('sh', ['-ec', 'test ! -e /var/run/docker.sock; test ! -e /opt/stack-bench; test ! -e /fixture; '
      + 'test ! -e /app/probe-owned.mjs; test ! -e /app/snapshot.mjs; '
      + 'test -z "$STACK_BENCH_LEASE_TOKEN$OPENAI_API_KEY$ANTHROPIC_API_KEY"; '
      + 'test ! -w /usr/bin/node; test ! -r /run/application']);
    developer('npm', ['ci', '--ignore-scripts', '--no-audit', '--no-fund']);
    developer('node', ['prepare-auth.mjs']);
    developer('node', ['node_modules/convex/bin/main.js', 'dev', '--once', '--typecheck', 'disable']);
    evidence.deployedSource = sourceHashes();
    assert.deepEqual(evidence.deployedSource, evidence.submittedSource, 'Native deployment must retain submitted application source');
    const generatedAuth = readFileSync(join(app, 'convex/auth.config.js'), 'utf8');
    assert.equal(generatedAuth, 'export default { providers: [{ domain: process.env.CONVEX_SITE_URL, applicationID: "convex" }] };\n');
    evidence.generatedAuthConfiguration = { source: generatedAuth, sha256: createHash('sha256').update(generatedAuth).digest('hex') };
    evidence.deployment = 'npm install, local auth setup and native Convex CLI deploy ran as UID 10001 in the owned coding namespace';
    const browser = active.resources.browserContainer!.id;
    attemptDocker(['exec', browser, 'mkdir', '-p', '/tmp/convex-owned']);
    const archive = execFileSync('tar', ['-C', app, '-cf', '-', 'node_modules', 'package.json'], { maxBuffer: 128 * 1024 * 1024 });
    execFileSync('docker', ['exec', '-i', browser, 'tar', '--no-same-owner', '-C', '/tmp/convex-owned', '-xf', '-'], { input: archive });
    for (const name of ['probe-owned.mjs', 'snapshot.mjs']) {
      attemptDocker(['exec', '-i', browser, 'sh', '-c', `cat > /tmp/convex-owned/${name}`], readFileSync(join(fixture, name), 'utf8'));
    }
    attemptDocker(['exec', '-i', browser, 'sh', '-c', 'umask 077; cat > /tmp/convex-owned/owned-private.json'], JSON.stringify(settings));
    try { attemptDocker(['exec', '-w', '/tmp/convex-owned', browser, 'node', 'probe-owned.mjs']); }
    finally {
      evidence.native = JSON.parse(attemptDocker(['exec', browser, 'cat', '/tmp/convex-owned/owned-probe.json']));
    }
    const reused = JSON.parse(prepare().trim());
    assert.equal(reused.identity, (evidence.prepared as { identity: string }).identity);
    evidence.reusedSameCodingContainer = true;
    evidence.activeLease = publicBackendLease(readBackendLease(leasePath));
    evidence.result = 'passed';
  } catch (error) {
    evidence.result = 'failed'; evidence.error = error instanceof Error ? error.message : String(error); failure = error;
  } finally {
    try {
      if (existsSync(leasePath)) {
        assert.equal(releaseConvex(leasePath, lease.ownershipToken), true);
        const final = readBackendLease(leasePath);
        evidence.finalLease = publicBackendLease(final);
        assert.equal(final.state, 'released');
        if (final.resources.buildContainer) assert(final.resources.buildContainer.workspaceHandedBackAt);
        assert(final.resources.locks.every(lock => lock.releasedAt && !existsSync(lock.path)));
        for (const volume of volumes) assert.throws(() => attemptDocker(['volume', 'inspect', volume]), /no such volume/i);
        evidence.anonymousVolumeCleanup = volumes.length;
        rmSync(root, { recursive: true });
        evidence.workspaceRemoved = !existsSync(root);
      }
    } catch (error) {
      evidence.result = 'failed'; evidence.cleanupError = error instanceof Error ? error.message : String(error); failure ??= error;
    } finally { save(); }
  }
  if (failure) throw failure;
});
