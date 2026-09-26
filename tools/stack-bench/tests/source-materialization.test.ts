import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { restoreRepairSource } from '../src/runtime/source-materialization.js';
import { hashAppSource, snapshotAppSource, restoreAppSource } from '../src/runtime/source-snapshot.js';

test('rejected installed dependency changes cannot survive accepted source materialization', { timeout: 60_000 }, async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dependency-rollback-'));
  try {
    const app = join(root, 'app'), accepted = join(root, 'accepted'), dependency = join(root, 'dependency');
    mkdirSync(dependency);
    mkdirSync(join(app, 'vendor'), { recursive: true });
    writeFileSync(join(dependency, 'package.json'), JSON.stringify({ name: 'rollback-dependency', version: '1.0.0' }));
    writeFileSync(join(dependency, 'index.js'), 'module.exports = "accepted";\n');
    const npm = (cwd: string, args: string[]) => execFileSync(process.platform === 'win32' ? process.execPath : 'npm',
      [...(process.platform === 'win32' ? [join(dirname(process.execPath), 'node_modules/npm/bin/npm-cli.js')] : []),
        ...args, '--offline', '--ignore-scripts', '--no-audit', '--no-fund', '--cache', join(root, 'cache')],
      { cwd, encoding: 'utf8', stdio: 'pipe', timeout: 15_000 });
    npm(dependency, ['pack', '--pack-destination', root]);
    copyFileSync(join(root, 'rollback-dependency-1.0.0.tgz'), join(app, 'vendor', 'dependency.tgz'));
    writeFileSync(join(app, 'package.json'), JSON.stringify({ dependencies: { 'rollback-dependency': 'file:vendor/dependency.tgz' } }));
    writeFileSync(join(app, 'server.cjs'), 'process.stdout.write(require("rollback-dependency"));\n');
    writeFileSync(join(app, 'start.sh'), '#!/bin/sh\nset -eu\nnpm ci --offline --ignore-scripts\nnode server.cjs\n');
    npm(app, ['install', '--package-lock-only']);
    npm(app, ['ci']);
    snapshotAppSource(app, accepted);
    const execute = () => execFileSync(process.execPath, [join(app, 'server.cjs')], { encoding: 'utf8' });
    assert.equal(execute(), 'accepted');
    writeFileSync(join(app, 'node_modules', 'rollback-dependency', 'index.js'), 'module.exports = "rejected";\n');
    const abandoned = join(app, 'rejected-layout', 'node_modules');
    mkdirSync(abandoned, { recursive: true });
    writeFileSync(join(abandoned, 'extra.js'), 'rejected');
    assert.equal(hashAppSource(app).sha256, hashAppSource(accepted).sha256);
    assert.equal(execute(), 'rejected', 'control must change executed code without changing accepted source');
    const events: string[] = [];
    await restoreRepairSource(accepted, app, { backend: 'postgres', app, port: 6573, probe: '' }, async (_spec, mode) => {
      events.push(mode ?? 'restart');
      if (mode === 'start') {
        assert.equal(existsSync(join(app, 'node_modules')), false);
        assert.equal(existsSync(abandoned), false);
        npm(app, ['ci']);
        assert.equal(execute(), 'accepted');
      }
    }, () => { events.push('reset'); });
    assert.deepEqual(events, ['stop', 'reset', 'start']);
    assert.equal(execute(), 'accepted');
    assert.equal(hashAppSource(app).sha256, hashAppSource(accepted).sha256);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('clean dependency restore removes nested links without touching external packages', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dependency-link-'));
  try {
    const app = join(root, 'app'), accepted = join(root, 'accepted'), outside = join(root, 'outside');
    mkdirSync(join(accepted, 'server'), { recursive: true });
    writeFileSync(join(accepted, 'server', 'index.js'), 'accepted');
    mkdirSync(outside);
    writeFileSync(join(outside, 'dependency.js'), 'external');
    for (const location of ['server', 'abandoned']) {
      mkdirSync(join(app, location), { recursive: true });
      symlinkSync(outside, join(app, location, 'node_modules'), process.platform === 'win32' ? 'junction' : 'dir');
    }
    restoreAppSource(accepted, app, { cleanDependencies: true });
    assert.equal(existsSync(join(app, 'server', 'node_modules')), false);
    assert.equal(existsSync(join(app, 'abandoned')), false);
    assert.equal(readFileSync(join(outside, 'dependency.js'), 'utf8'), 'external');
  } finally { rmSync(root, { recursive: true, force: true }); }
});
