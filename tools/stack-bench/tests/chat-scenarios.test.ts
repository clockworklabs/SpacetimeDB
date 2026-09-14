import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

const directory = join(STACK_BENCH_ROOT, 'tracks', 'chat', 'scenarios');
const load = (name: string) => compileScenarioDefinition(
  JSON.parse(readFileSync(join(directory, name), 'utf8')), { source: name });
const feature = (name: string, id: number) => load(name).features.find(value => value.id === id)!;

for (const name of readdirSync(directory).filter(name => name.endsWith('.json'))) {
  test(`chat scenario ${name} compiles, including inactive diagnostics`, () => { load(name); });
}

test('chat distinguishes eventual transitions from continued absence', () => {
  const typing = feature('01-basic-chat.json', 3).criteria;
  assert(typing.find(value => value.id === 'indicator-expires')!.steps
    .some(step => step.do === 'waitUntilAbsent'));
  assert(typing.find(value => value.id === 'scoped-to-room')!.steps
    .some(step => step.do === 'expect' && step.absent));
});

test('private-room setup enters the room before sending and replay checks delivery', () => {
  for (const id of [201, 202]) {
    const steps = feature('02-features.json', id).setup;
    assert(steps.findIndex(step => step.do === 'enterRoom') < steps.findIndex(step => step.do === 'send'));
  }
  const steps = feature('02-invariants.json', 212).criteria.find(value => value.id === '212b')!.steps;
  assert.equal(steps[0]!.do, 'replayAs');
  assert.equal(steps[1]!.do, 'expectReplayCompleted');
});

test('reconnect ownership uses authority and receipt stability has two readers', () => {
  const ownership = feature('01-invariants.json', 102).criteria
    .find(value => value.id === 'own-content-still-owned-after-reconnect')!.steps;
  assert(ownership.some(step => step.do === 'click' && step.testid === 'member-remove'));
  assert(ownership.some(step => step.do === 'waitUntilAbsent' && step.actor === 'bob'));
  const stability = feature('01-invariants.json', 105);
  assert.equal(stability.actors!.length, 3);
  assert.deepEqual(stability.criteria[0]!.steps.filter(step => step.do === 'expect').map(step => step.contains), ['Bob', 'Carol']);
});

test('inactive pin-cap control oversubscribes three slots with four replays', () => {
  const steps = feature('01-contention-wip.json', 106).criteria
    .find(value => value.id === 'pin-cap-holds-under-concurrency')!.steps;
  const replay = steps.find(step => step.do === 'replayConcurrently')!;
  assert.equal(replay.actors!.length, 4);
  assert(steps.some(step => step.do === 'click' && step.actor === 'dave' && step.in?.contains === 'PIN-4'));
});

test('resync checks acknowledged history that predates the disconnect', () => {
  const resilience = feature('01-delivery.json', 202);
  assert(resilience.setup.some(step => step.do === 'expectAllPresent' && step.prefix === 'BEFORE' && step.count === 2));
  const reconnect = resilience.criteria.find(value => value.id === 'no-duplication-after-resync')!;
  assert(reconnect.steps.some(step => step.do === 'expectAllPresent' && step.prefix === 'BEFORE' && step.count === 2));
});
