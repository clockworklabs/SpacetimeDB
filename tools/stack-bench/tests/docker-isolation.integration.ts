import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { attemptNetworkRules } from '../src/runtime/docker-network.js';

// Explicit opt-in: this creates short-lived model-free Docker resources.
test('attempt namespace permits owned services and app replies while denying host and other attempts', {
  skip: process.env.STACK_BENCH_DOCKER_ISOLATION_TEST !== '1', timeout: 90_000,
}, async t => {
  const helperImage = process.env.STACK_BENCH_NETWORK_HELPER_IMAGE;
  const appImage = process.env.STACK_BENCH_NETWORK_PROBE_IMAGE;
  const controllerImage = process.env.STACK_BENCH_NETWORK_CONTROLLER_IMAGE;
  assert.ok(helperImage && appImage && controllerImage,
    'supply existing nftables helper, Node probe, and controller image IDs');
  const prefix = `stack-bench-isolation-probe-${randomBytes(6).toString('hex')}`;
  const containers: string[] = [];
  const networks: string[] = [];
  const volumes: string[] = [];
  function docker(args: string[], input?: string): string {
    const result = spawnSync('docker', args, { encoding: 'utf8', input, timeout: 15_000 });
    assert.equal(result.status, 0, `${args.slice(0, 3).join(' ')}: ${result.stdout}\n${result.stderr || result.error}`);
    return result.stdout.trim();
  }
  const run = (name: string, args: string[], script: string): string => {
    const id = docker(['create', '--name', `${prefix}-${name}`, '--cap-drop', 'ALL',
      '--security-opt', 'no-new-privileges:true', ...args, '--entrypoint', 'node', appImage!, '-e', script]);
    containers.push(id);
    docker(['start', id]);
    return id;
  };
  const serve = (ports: number[]) => `const h=require('node:http'); for(const p of ${JSON.stringify(ports)})`
    + `h.createServer((q,s)=>s.end('ok')).listen(p,'::',function(){console.log(this.address().port)});`;
  const endpoint = (id: string, network: string): { IPAddress: string; Gateway: string; GlobalIPv6Address: string } =>
    JSON.parse(docker(['inspect', '--format', '{{json .NetworkSettings.Networks}}', id]))[network];
  const probe = (id: string, urls: string[]): boolean[] => JSON.parse(docker(['exec', id, 'node', '-e',
    `Promise.all(${JSON.stringify(urls)}.map(async u=>{try{await fetch(u,{signal:AbortSignal.timeout(1500)});`
      + `return true}catch{return false}})).then(r=>console.log(JSON.stringify(r)))`]));
  try {
    const trackedBeforeFailure = containers.length;
    assert.throws(() => run('failed-start', ['--network', `container:${prefix}-absent`], serve([18083])),
      /No such container/);
    assert.equal(containers.length, trackedBeforeFailure + 1, 'failed starts must retain the created ID for cleanup');
    const first = `${prefix}-a`;
    const second = `${prefix}-b`;
    for (const name of [first, second]) networks.push(docker(['network', 'create', '--ipv6', name]));
    const app = run('app', ['--network', first, '-p', '127.0.0.1::18083'], serve([18083]));
    const own = run('own', ['--network', first], serve([18080, 18081]));
    const other = run('other', ['--network', second], serve([18080]));
    const host = run('host', ['--network', 'host'], serve([0]));
    const broker = run('broker', ['--network', `container:${app}`],
      "require('node:http').createServer((q,s)=>s.end('ok')).listen(18084,'127.0.0.1')");
    assert.ok(broker);
    const ownAddress = endpoint(own, first).IPAddress;
    const otherAddress = endpoint(other, second).IPAddress;
    const hostAddress = endpoint(app, first).Gateway;
    const hostPort = Number(docker(['logs', host]));
    assert.ok(hostPort > 0, 'host listener became ready');
    const ownUrl = `http://${ownAddress}:18080`;
    const ipv6Url = `http://[${endpoint(own, first).GlobalIPv6Address}]:18080`;
    const deniedPortUrl = `http://${ownAddress}:18081`;
    const hostUrl = `http://${hostAddress}:${hostPort}`;
    const otherUrl = `http://${otherAddress}:18080`;
    assert.deepEqual(probe(app, [ownUrl, deniedPortUrl, hostUrl, ipv6Url, 'http://127.0.0.1:18084']),
      [true, true, true, true, true], 'positive controls before firewall');
    assert.deepEqual(probe(other, ['http://127.0.0.1:18080']), [true], 'other attempt is live');
    const rules = attemptNetworkRules({ services: [{ address: ownAddress, port: 18080 }],
      hostAddresses: [hostAddress] });
    docker(['run', '--rm', '-i', '--network', `container:${app}`, '--cap-drop', 'ALL',
      '--cap-add', 'NET_ADMIN', '--security-opt', 'no-new-privileges:true', '--read-only',
      '--entrypoint', 'nft', helperImage, '-f', '-'], rules);
    assert.deepEqual(probe(app, [ownUrl, deniedPortUrl, hostUrl, otherUrl, ipv6Url, 'http://127.0.0.1:18084']),
      [true, false, false, false, false, true], 'owned backend/broker allowed, live forbidden endpoints denied');
    assert.deepEqual(probe(app, ['https://api.anthropic.com', 'https://registry.npmjs.org']),
      [true, true], 'public provider and registry HTTPS routes work without model calls');
    const binding = docker(['port', app, '18083/tcp']);
    const response = await fetch(`http://${binding}`, { signal: AbortSignal.timeout(1500) });
    assert.equal(await response.text(), 'ok', 'published app replies reach host grader');
    const volume = docker(['volume', 'create', `${prefix}-private`]);
    volumes.push(volume);
    const privateDirectory = docker(['volume', 'inspect', '--format', '{{.Mountpoint}}', volume]);
    const brokerProcess = fileURLToPath(new URL('../container/credential-broker-process.js', import.meta.url));
    const brokerProof = `
      const { startCredentialBroker, stopCredentialBroker, credentialBrokerDiagnostics } =
        await import('/opt/stack-bench/dist/container/credential-broker-process.js');
      const { spawnSync } = await import('node:child_process');
      const { existsSync, writeFileSync } = await import('node:fs');
      const assert = (await import('node:assert/strict')).default;
      const privateDirectory = ${JSON.stringify(privateDirectory)};
      const imageId = ${JSON.stringify(controllerImage)};
      const networkContainerId = ${JSON.stringify(app)};
      let recorded;
      const credential = 'model-free-probe-fake-credential';
      const options = { networkMode: 'bridge', deadlineMs: 10_000, model: 'test-model',
        docker: { imageId, networkContainerId, privateDirectory,
          name: ${JSON.stringify(`${prefix}-real-broker`)}, creationToken: '${randomBytes(16).toString('hex')}',
          onCreated(container) {
            recorded = container;
            assert.equal(spawnSync('docker', ['inspect', '--format', '{{.State.Running}}', container.id],
              { encoding: 'utf8' }).stdout.trim(), 'false');
            writeFileSync(privateDirectory + '/broker-authority.json', JSON.stringify(container), { mode: 0o600 });
          }
        }
      };
      const broker = await startCredentialBroker({ mode: 'api-key', credential }, options);
      try {
        assert.equal(recorded.id, broker.container.id);
        const inspection = JSON.parse(spawnSync('docker', ['inspect', broker.container.id],
          { encoding: 'utf8' }).stdout)[0];
        assert.equal(JSON.stringify(inspection.Config).includes(credential), false);
        assert.equal(inspection.HostConfig.NetworkMode, 'container:' + networkContainerId);
        assert.equal(inspection.Mounts.length, 1);
        assert.equal((await fetch(broker.baseUrl)).status, 401);
        assert.equal((await fetch(broker.baseUrl, { headers: { authorization: 'Bearer ' + broker.sessionToken } })).status, 404);
        const appRead = spawnSync('docker', ['exec', networkContainerId, 'test', '-e', broker.root]);
        assert.notEqual(appRead.status, 0, 'coding container cannot read provider files');
      } finally {
        const ledger = await stopCredentialBroker(broker);
        assert.equal(ledger.complete, true);
        assert.equal(ledger.spentUsd, 0);
        assert.equal(credentialBrokerDiagnostics(broker).termination.exited, true);
        assert.equal(existsSync(broker.ledgerPath), true);
        assert.equal(existsSync(broker.root + '/config.json'), false);
        assert.equal(existsSync(broker.root + '/ready.json'), false);
      }
      assert.notEqual(spawnSync('docker', ['inspect', recorded.id]).status, 0);
      console.log('sidecar identity, loopback authentication, private mount, receipt drain, and removal passed');
    `;
    const controller = docker(['create', '--name', `${prefix}-controller`, '--network', `container:${app}`,
      '--mount', 'type=bind,src=/var/run/docker.sock,dst=/var/run/docker.sock',
      '--mount', `type=volume,src=${volume},dst=${privateDirectory}`,
      '--mount', `type=bind,src=${brokerProcess},dst=/opt/stack-bench/dist/container/credential-broker-process.js,readonly`,
      '--entrypoint', 'node', controllerImage, '--input-type=module', '-e', brokerProof]);
    containers.push(controller);
    t.diagnostic(docker(['start', '--attach', controller]));
    assert.equal(docker(['inspect', '--format', '{{.State.ExitCode}}', controller]), '0');
    const browserName = `${prefix}-browser`;
    const browser = docker(['create', '--name', browserName, '--network', `container:${app}`,
      '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges:true', '--read-only',
      '--env', 'HOME=/tmp', '--env', 'XDG_CONFIG_HOME=/tmp/.config', '--env', 'XDG_CACHE_HOME=/tmp/.cache',
      '--tmpfs', '/tmp:size=256m,mode=1777', '--shm-size', '256m', '--pids-limit', '128',
      '--memory', '1g', '--entrypoint', 'sleep', controllerImage, 'infinity']);
    containers.push(browser);
    docker(['start', browser]);
    const browserProof = `
      const assert = (await import('node:assert/strict')).default;
      const { chromium } = await import('playwright');
      const { createBackendLease, writeBackendLease } = await import('./dist/src/runtime/backend-lease.js');
      const { attemptBrowserLaunchOptions } = await import('./dist/container/browser-pipe.js');
      const lease = createBackendLease({runId:'browser-probe', backend:'postgres', track:'ecommerce',
        runIndex:0, database:'probe', container:{name:'probe-anchor',id:${JSON.stringify(app)}}});
      lease.resources.browserContainer = {name:${JSON.stringify(browserName)},id:${JSON.stringify(browser)},
        image:${JSON.stringify(controllerImage)},owned:true,networkMode:'container:'+${JSON.stringify(app)}};
      process.env.STACK_BENCH_LEASE='/tmp/browser-lease.json';
      process.env.STACK_BENCH_LEASE_TOKEN=lease.ownershipToken;
      writeBackendLease(process.env.STACK_BENCH_LEASE,lease);
      for (const shared of [false,true]) {
        const server = shared ? await chromium.launchServer({...attemptBrowserLaunchOptions(),timeout:5000}) : null;
        const browser = server ? await chromium.connect(server.wsEndpoint())
          : await chromium.launch({...attemptBrowserLaunchOptions(),timeout:5000});
        try {
          const page = await browser.newPage();
          await page.goto('http://127.0.0.1:18083',{timeout:5000});
          assert.equal(await page.textContent('body'),'ok');
          assert.deepEqual(await page.evaluate(async urls => Promise.all(urls.map(async url => {
            try { await fetch(url,{signal:AbortSignal.timeout(1000)});return true; } catch { return false; }
          })), ${JSON.stringify([hostUrl, otherUrl, ipv6Url])}), [false,false,false]);
          assert.ok((await page.screenshot()).length > 100);
        } finally { await browser.close(); await server?.close(); }
      }
      console.log('browser pipe launch, shared driver, screenshots, and page-origin denial passed');
    `;
    const browserController = docker(['create', '--name', `${prefix}-browser-controller`, '--network', 'none',
      '--mount', 'type=bind,src=/var/run/docker.sock,dst=/var/run/docker.sock',
      '--entrypoint', 'node', controllerImage, '--input-type=module', '-e', browserProof]);
    containers.push(browserController);
    t.diagnostic(docker(['start', '--attach', browserController]));
    assert.equal(docker(['inspect', '--format', '{{.State.ExitCode}}', browserController]), '0');
    const table = docker(['run', '--rm', '--network', `container:${app}`, '--cap-drop', 'ALL',
      '--cap-add', 'NET_ADMIN', '--entrypoint', 'nft', helperImage, 'list', 'table', 'inet', 'stack_bench']);
    t.diagnostic(JSON.stringify({ docker: docker(['version', '--format', '{{.Server.Version}}']),
      helperImage, appImage, firewall: 'nftables in attempt namespace', publicEgress: 'IPv4 TCP 80/443', table }));
  } finally {
    // IDs came from successful creations in this test. Never discover/delete by prefix.
    const errors: unknown[] = [];
    for (const id of containers.reverse()) {
      try { docker(['rm', '-f', id]); } catch (error) { errors.push(error); }
    }
    for (const id of networks.reverse()) {
      try { docker(['network', 'rm', id]); } catch (error) { errors.push(error); }
    }
    for (const name of volumes.reverse()) {
      try { docker(['volume', 'rm', name]); } catch (error) { errors.push(error); }
    }
    assert.deepEqual(errors, [], 'all owned Docker resources were removed');
  }
});
