import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { execFileSync, spawn } from 'node:child_process';
import test from 'node:test';
import { validateProject } from '../container/spacetime-dev.js';

test('managed development validates the assigned target and application paths', () => {
  const root = mkdtempSync(join(tmpdir(), 'managed-dev-'));
  const config = { server: 'http://localhost:3210', database: 'app', 'module-path': 'module',
    generate: [{ language: 'typescript', 'out-dir': 'client/bindings' }] };
  mkdirSync(join(root, 'module'));
  const save = (value: unknown) => writeFileSync(join(root, 'spacetime.json'), JSON.stringify(value));
  try {
    save(config); assert.doesNotThrow(() => validateProject(root, config.server, 'app'));
    assert.throws(() => validateProject(root, config.server, 'other'), /supplied/);
    save({ ...config, generate: [] }); assert.throws(() => validateProject(root, config.server, 'app'), /generate/);
    save({ ...config, 'module-path': '../outside' }); assert.throws(() => validateProject(root, config.server, 'app'), /inside/);
    save(config); writeFileSync(join(root, 'spacetime.local.json'), '{"database":"other"}');
    assert.throws(() => validateProject(root, config.server, 'app'), /supplied/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('managed watcher starts once, reports readiness, stops and exposes startup failures',
  { skip: process.platform !== 'linux' }, async () => {
    const root = mkdtempSync(join(tmpdir(), 'managed-dev-'));
    const state = join(root, 'state'); mkdirSync(state); mkdirSync(join(root, 'module'));
    const cli = join(root, 'cli');
    writeFileSync(join(root, 'spacetime.json'), JSON.stringify({ server: 'http://localhost:3210',
      database: 'app', 'module-path': 'module', generate: [{ language: 'typescript', 'out-dir': 'bindings' }] }));
    writeFileSync(cli, '#!/usr/bin/env node\nconst fs=require("node:fs"); fs.writeFileSync(process.argv[process.argv.indexOf("--ready-file")+1],"ready"); setInterval(()=>{},1000);', { mode: 0o755 });
    const args = ['-w', '15', join(state, 'command.lock'), process.execPath,
      resolve('dist/container/spacetime-dev.js'), root, state, cli, 'http://localhost:3210', 'app'];
    const run = (command: string) => execFileSync('flock', [...args, command], { encoding: 'utf8', timeout: 20000 });
    try {
      const start = () => new Promise<string>((done, reject) => {
        const child = spawn('flock', [...args, 'start']); let output = '';
        child.stdout.on('data', data => { output += data; });
        child.once('error', reject); child.once('exit', code => code === 0 ? done(output) : reject(new Error(output)));
      });
      const results = await Promise.all([start(), start()]);
      assert(results.every(value => value.includes('Running')));
      const before = readFileSync(join(state, 'process.json'), 'utf8');
      assert.match(run('start'), /Running/);
      assert.equal(readFileSync(join(state, 'process.json'), 'utf8'), before);
      assert.match(run('stop'), /Stopped/); assert.match(run('status'), /Not running/);
      writeFileSync(cli, '#!/bin/sh\necho compile-failed >&2\nexit 1\n');
      assert.throws(() => run('start'), /Watcher exited/);
      assert.match(readFileSync(join(state, 'watcher.log'), 'utf8'), /compile-failed/);
    } finally { run('stop'); rmSync(root, { recursive: true, force: true }); }
  });
