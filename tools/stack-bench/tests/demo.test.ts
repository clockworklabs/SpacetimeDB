import assert from 'node:assert/strict';
import { readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import test from 'node:test';
import { demoConfiguration } from '../appliance/demo.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('demo build context contains the coding image Dockerfile and browser dependencies', () => {
  const appliance = join(STACK_BENCH_ROOT, 'appliance');
  const compose = readFileSync(join(appliance, 'demo.compose.yaml'), 'utf8');
  const build = compose.match(/build-image:[\s\S]*?context:\s*(\S+)\s+dockerfile:\s*(\S+)/);
  assert.ok(build, 'demo must declare its coding-image build context');
  const context = resolve(appliance, build[1]!);
  assert.ok(statSync(join(context, build[2]!)).isFile());
  for (const file of ['package.json', 'package-lock.json']) {
    assert.ok(statSync(join(context, 'browser-tools', file)).isFile());
  }
});

test('demo uses three workers, isolated release dependencies and only a model-free trial', () => {
  const digest = 'a'.repeat(64);
  const config = demoConfiguration(`STACK_BENCH_CONTROLLER_IMAGE=sha256:${digest}\n`
    + 'STACK_BENCH_STATE_ROOT=/var/lib/docker/volumes/stack-bench-state/_data\n'
    + 'STACK_BENCH_RELEASE_MANIFEST=\n'
    + 'STACK_BENCH_BUILD_IMAGE=example=value\n', { UNRELATED_SECRET: 'not-saved' });
  assert.equal(config.env.STACK_BENCH_BUILD_IMAGE, 'example=value');
  assert.equal(config.env.STACK_BENCH_RELEASE_DEPS_VOLUME, 'stack-bench-release-deps-aaaaaaaaaaaa');
  assert.deepEqual(config.dashboard.slice(-5), ['--profile', 'dashboard', 'up', '-d', 'dashboard']);
  assert.deepEqual(config.campaign.slice(-6), ['controller', 'campaign', 'trial', 'plans/demo.json', '--out',
    'campaigns/demo-aaaaaaaaaaaa']);
  assert.doesNotMatch(config.savedEnvironment, /not-saved|UNRELATED_SECRET/);
  const other = demoConfiguration(`STACK_BENCH_CONTROLLER_IMAGE=sha256:${'b'.repeat(64)}\n`, {});
  assert.notEqual(other.output, config.output);
  assert.notEqual(other.env.STACK_BENCH_RELEASE_DEPS_VOLUME, config.env.STACK_BENCH_RELEASE_DEPS_VOLUME);
});

test('demo refuses an unresolved image or malformed setup environment', () => {
  assert.throws(() => demoConfiguration('STACK_BENCH_CONTROLLER_IMAGE=local:tag\n', {}), /digest/);
  assert.throws(() => demoConfiguration('invalid\n', {}), /environment/);
});
