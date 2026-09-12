import assert from 'node:assert/strict';
import test from 'node:test';
import { tmpdir } from 'node:os';
import { mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { createBackendLease, publicBackendLease, readBackendLease, validateBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import { attemptBrowserLaunchOptions } from '../container/browser-pipe.js';

import { attemptNetworkRules, dockerHostGatewayArguments, dockerHostServiceAddress,
  installAttemptFirewall, createAttemptNetwork, createAttemptContainer } from '../src/runtime/docker-network.js';

test('only the authenticated owned browser uses its private shared memory', () => {
  const root = mkdtempSync(join(tmpdir(), 'browser-shm-'));
  const saved = { lease: process.env.STACK_BENCH_LEASE,
    token: process.env.STACK_BENCH_LEASE_TOKEN, appliance: process.env.STACK_BENCH_APPLIANCE };
  try {
    delete process.env.STACK_BENCH_LEASE;
    delete process.env.STACK_BENCH_APPLIANCE;
    assert.deepEqual(attemptBrowserLaunchOptions(), {});
    const lease = createBackendLease({ backend: 'postgres', track: 'ecommerce', runIndex: 0,
      runId: 'browser-shm', database: 'browser_shm' });
    const anchor = 'b'.repeat(64);
    lease.resources.container = { name: 'anchor', id: anchor, owned: true };
    lease.resources.network = { name: 'attempt', id: 'c'.repeat(64), namespaceContainerId: anchor,
      hostAddresses: ['172.20.0.1'], services: [], firewallSha256: null, firewallInstalledAt: null };
    lease.resources.browserContainer = { name: 'browser', id: 'a'.repeat(64), owned: true,
      image: `sha256:${'d'.repeat(64)}`, networkMode: `container:${anchor}` };
    process.env.STACK_BENCH_LEASE = join(root, 'lease.json');
    process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
    writeBackendLease(process.env.STACK_BENCH_LEASE, lease);
    const options = attemptBrowserLaunchOptions();
    assert.match(options.executablePath ?? '', /browser-pipe\.js$/);
    assert.deepEqual(options.ignoreDefaultArgs, ['--disable-dev-shm-usage']);
    process.env.STACK_BENCH_LEASE_TOKEN = 'wrong-token';
    assert.throws(() => attemptBrowserLaunchOptions(), /token/);
  } finally {
    for (const [key, value] of Object.entries({ STACK_BENCH_LEASE: saved.lease,
      STACK_BENCH_LEASE_TOKEN: saved.token, STACK_BENCH_APPLIANCE: saved.appliance })) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
    rmSync(root, { recursive: true, force: true });
  }
});

test('attempt cache attachment preserves the shared gateway and binds only its exact service address', () => {
  const root = mkdtempSync(join(tmpdir(), 'cache-attachment-'));
  const image = process.env.STACK_BENCH_CONTROLLER_IMAGE_ID;
  process.env.STACK_BENCH_CONTROLLER_IMAGE_ID = `sha256:${'f'.repeat(64)}`;
  try {
    const lease = createBackendLease({ backend: 'postgres', track: 'chat', runIndex: 0,
      runId: 'cache-attachment', database: 'cache_attachment' });
    const network = 'a'.repeat(64), cache = 'b'.repeat(64), anchor = 'c'.repeat(64);
    lease.resources.container = { name: 'anchor', id: anchor, owned: true };
    lease.resources.network = { name: 'attempt', id: network, namespaceContainerId: anchor,
      hostAddresses: ['172.20.0.1'], services: [], firewallSha256: null, firewallInstalledAt: null };
    const path = join(root, 'lease.json');
    writeBackendLease(path, lease);
    const commands: string[][] = [];
    const active = installAttemptFirewall(path, lease, (args, input) => {
      commands.push(args);
      if (args[0] === 'inspect') return args[2] === '{{.Id}}' ? cache : JSON.stringify({
        original: { NetworkID: 'd'.repeat(64), IPAddress: '172.31.0.4', GwPriority: 0 },
        attempt: { NetworkID: network, IPAddress: '172.20.0.3', GwPriority: -1 },
      });
      if (args[0] === 'run') assert.match(input ?? '', /ip daddr 172\.20\.0\.3 tcp dport 4873 accept/);
      return '';
    });
    assert.deepEqual(commands.filter(args => args[0] === 'network'),
      [['network', 'connect', '--gw-priority', '-1', network, cache]]);
    assert.equal(active.resources.network?.cacheContainerId, cache);
    assert.deepEqual(active.resources.network?.services, [{ address: '172.20.0.3', port: 4873 }]);
  } finally {
    if (image === undefined) delete process.env.STACK_BENCH_CONTROLLER_IMAGE_ID;
    else process.env.STACK_BENCH_CONTROLLER_IMAGE_ID = image;
    rmSync(root, { recursive: true, force: true });
  }
});

test('lease binds sidecars to the exact backend namespace and excludes creation authority from evidence', () => {
  const lease = createBackendLease({ backend: 'spacetime', track: 'chat', runIndex: 0, runId: 'network-proof',
    serverUri: 'http://127.0.0.1:3210', module: 'proof', dataDir: tmpdir() });
  const anchor = 'a'.repeat(64);
  lease.resources.container = { name: 'anchor', id: anchor, owned: true };
  lease.resources.network = { name: 'attempt', id: 'b'.repeat(64), namespaceContainerId: anchor,
    hostAddresses: ['172.20.0.1'], services: [], firewallSha256: null, firewallInstalledAt: null };
  lease.resources.browserContainer = { name: 'browser', id: 'c'.repeat(64), image: `sha256:${'d'.repeat(64)}`,
    owned: true, networkMode: `container:${anchor}` };
  lease.resources.creationIntents = { browser: { name: 'browser', creationToken: 'e'.repeat(32) } };
  assert.equal(validateBackendLease(lease), lease);
  assert.equal('creationIntents' in publicBackendLease(lease).resources, false);
  assert.equal(lease.resources.creationIntents.browser?.creationToken, 'e'.repeat(32));
  lease.resources.browserContainer.networkMode = `container:${'f'.repeat(64)}`;
  assert.throws(() => validateBackendLease(lease), /outside the leased network namespace/);
});

test('attempt firewall limits private services to their port and rejects unsafe rule inputs', () => {
  const rules = attemptNetworkRules({ services: [{ address: '172.20.0.3', port: 5432 }],
    hostAddresses: ['172.20.0.1', '203.1.2.3'] });
  assert.match(rules, /ip daddr 172\.20\.0\.3 tcp dport 5432 accept/);
  assert.match(rules, /203\.1\.2\.3/);
  assert.match(rules, /meta nfproto ipv4 tcp dport \{ 80, 443 \} accept/);
  assert.ok(rules.indexOf('ct state established,related accept') < rules.indexOf('ip daddr {'));
  assert.ok(rules.indexOf('ip daddr {') < rules.indexOf('meta nfproto ipv4'));
  assert.match(rules, /policy drop/);
  assert.throws(() => attemptNetworkRules({ services: [], hostAddresses: [] }), /host addresses/);
  assert.throws(() => attemptNetworkRules({ services: [{ address: '172.20.0.1', port: 5432 }],
    hostAddresses: ['172.20.0.1'] }), /cannot be a host/);
  for (const address of ['backend', '127.0.0.1; accept', '::1']) {
    assert.throws(() => attemptNetworkRules({ services: [], hostAddresses: [address] }), /literal IPv4/);
  }
  assert.throws(() => attemptNetworkRules({ services: [{ address: '172.20.0.3', port: 0 }],
    hostAddresses: ['172.20.0.1'] }), /port is invalid/);
});

test('bridge containers bind the portable Docker host gateway alias', () => {
  assert.deepEqual(dockerHostGatewayArguments('bridge'),
    ['--add-host', 'host.docker.internal:host-gateway']);
  assert.deepEqual(dockerHostGatewayArguments('host'), []);
  assert.throws(() => dockerHostGatewayArguments('ambient'), /unsupported Docker network mode/);
});

test('host services use the address reachable from the selected network namespace', () => {
  assert.equal(dockerHostServiceAddress('bridge'), 'host.docker.internal');
  assert.equal(dockerHostServiceAddress('host'), '127.0.0.1');
  assert.throws(() => dockerHostServiceAddress('ambient'), /unsupported Docker network mode/);
});


test('attempt subnets avoid host routes and Docker IPAM, and retry only overlap races', () => {
  const header = 'Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT';
  for (const mode of ['race', 'host-route', 'exhausted', 'daemon-error'] as const) {
    const root = mkdtempSync(join(tmpdir(), 'attempt-subnet-'));
    try {
      const lease = createBackendLease({ backend: 'postgres', track: 'chat', runIndex: 0,
        runId: `subnet-${mode}`, database: 'subnet_test' });
      const path = join(root, 'lease.json');
      writeBackendLease(path, lease);
      const attempts: string[][] = [];
      const route = mode === 'host-route' ? 'eth0 0000000A 00000000 0001 0 0 0 00FFFFFF 0 0 0'
        : 'eth0 00000000 0100000A 0003 0 0 0 00000000 0 0 0';
      const reserved = mode === 'exhausted' ? ['10.0.0.0/8', '172.16.0.0/12', '192.168.0.0/16']
        : [mode === 'host-route' ? '10.0.1.0/29' : '10.0.0.0/29', 'fd00::/64'];
      const id = 'b'.repeat(64);
      const docker = (args: string[]) => {
        if (args[1] === 'ls') return 'existing';
        if (args[1] === 'inspect') return JSON.stringify([{ IPAM: { Config:
          args[2] === id ? [{ Subnet: attempts.at(-1)![5], Gateway: '10.0.0.17' }]
            : reserved.map(Subnet => ({ Subnet })) } }]);
        assert.equal(args[1], 'create');
        attempts.push(args);
        if (mode === 'daemon-error') throw Object.assign(new Error('permission denied'), { stderr: 'permission denied' });
        if (mode === 'race' && attempts.length === 1) {
          throw Object.assign(new Error('overlap'), { stderr: 'Error response from daemon: Pool overlaps with other one on this address space' });
        }
        return id;
      };
      const create = () => createAttemptNetwork(path, lease, docker, `${header}\n${route}\n`);
      if (mode === 'exhausted') {
        assert.throws(create, /No unused private IPv4/);
        assert.equal(attempts.length, 0);
      } else if (mode === 'daemon-error') {
        assert.throws(create, /permission denied/);
        assert.equal(attempts.length, 1);
      } else {
        const updated = create();
        assert.equal(updated.resources.network?.id, id);
        assert.deepEqual(attempts.map(args => args[5]), mode === 'race'
          ? ['10.0.0.8/29', '10.0.0.16/29'] : ['10.0.1.8/29']);
        assert.ok(attempts.every(args => args[7]!.includes(updated.resources.creationIntents!.network!.creationToken)));
      }
    } finally { rmSync(root, { recursive: true, force: true }); }
  }
});

for (const kind of ['backend', 'browser'] as const) {
  test(`${kind} start failure retains sanitized Docker diagnostics before cleanup`, () => {
    const root = mkdtempSync(join(tmpdir(), 'container-start-failure-'));
    try {
      const lease = createBackendLease({ backend: 'postgres', track: 'chat', runIndex: 0,
        runId: 'container-start-failure', database: 'container_start_failure' });
      const id = 'c'.repeat(64);
      const anchor = 'a'.repeat(64);
      lease.resources.container = { name: 'anchor', id: anchor, owned: true };
      lease.resources.network = { name: 'attempt', id: 'b'.repeat(64), namespaceContainerId: anchor,
        hostAddresses: [], services: [], firewallSha256: null, firewallInstalledAt: null };
      const path = join(root, 'lease.json');
      writeBackendLease(path, lease);
      const commands: string[][] = [];
      assert.throws(() => createAttemptContainer(path, lease, kind,
        `sha256:${'d'.repeat(64)}`, kind === 'backend' ? 'attempt' : `container:${anchor}`, [], args => {
          commands.push(args);
          if (args[0] === 'create') return id;
          if (args[0] === 'start') throw Object.assign(new Error('Command failed: docker start'),
            { status: 1, signal: null, stderr: Buffer.from('password="private value"'), stdout: '' });
          assert.deepEqual(args, ['inspect', '--format', '{{json .State}}', id]);
          return JSON.stringify({ Status: 'created', Error: 'OCI start failed: api_key="private key"',
            ExitCode: 128, OOMKilled: false });
        }), error => {
        const message = String(error);
        assert.match(message, /OCI start failed/);
        assert.match(message, /"status":1/);
        assert.match(message, /"exitCode":128/);
        assert.match(message, /"oomKilled":false/);
        assert.doesNotMatch(message, /private|value|key/);
        return true;
      });
      assert.equal(commands.filter(args => args[0] === 'start').length, 1, 'do not retry activation');
      const retained = readBackendLease(path);
      assert.equal((kind === 'backend' ? retained.resources.container : retained.resources.browserContainer)?.id, id);
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

  test(`${kind} creation enforces its own memory cap without swap`, () => {
    const root = mkdtempSync(join(tmpdir(), 'container-limits-'));
    try {
      const lease = createBackendLease({ backend: 'postgres', track: 'chat', runIndex: 0,
        runId: 'container-limits', database: 'container_limits' });
      const anchor = 'a'.repeat(64);
      lease.resources.container = { name: 'anchor', id: anchor, owned: true };
      lease.resources.network = { name: 'attempt', id: 'b'.repeat(64), namespaceContainerId: anchor,
        hostAddresses: ['172.20.0.1'], services: [], firewallSha256: null, firewallInstalledAt: null };
      const path = join(root, 'lease.json');
      writeBackendLease(path, lease);
      const commands: string[][] = [];
      createAttemptContainer(path, lease, kind, `sha256:${'d'.repeat(64)}`, `container:${anchor}`, [], args => {
        commands.push(args);
        if (args[0] === 'create') return 'c'.repeat(64);
        if (args[0] === 'inspect') return args[2] === '{{json .NetworkSettings.Networks}}' ? '{}' : '2026-09-10T00:00:00Z';
        return '';
      });
      const command = commands.find(args => args[0] === 'create')!;
      const memory = String((kind === 'browser' ? 2 : 1) * 1024 ** 3);
      assert.equal(command[command.indexOf('--memory') + 1], memory);
      assert.equal(command[command.indexOf('--memory-swap') + 1], memory);
      assert.equal(command[command.indexOf('--cpus') + 1], '1');
      assert.equal(command[command.indexOf('--pids-limit') + 1], kind === 'browser' ? '512' : '256');
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
}
