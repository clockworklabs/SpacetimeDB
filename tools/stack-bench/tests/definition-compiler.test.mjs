import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import {
  ACTION_IDS,
  DEFINITION_SCHEMA_VERSION,
  compileScenarioDefinition,
  compileTrackManifest,
} from '../definition-compiler.mjs';
import { ACTION_REGISTRY } from '../action-catalog.mjs';
import { BROWSER_ACTION_IDS } from '../browser-action-executors.mjs';
import { ACTOR_TRANSPORT_ACTION_IDS } from '../actor-transport-action-executors.mjs';
import { LIFECYCLE_CONCURRENCY_ACTION_IDS } from '../lifecycle-concurrency-action-executors.mjs';
import { TRACKS_DIR } from '../tracks.mjs';

function currentDefinitions() {
  const definitions = [];
  for (const track of readdirSync(TRACKS_DIR)) {
    const root = join(TRACKS_DIR, track);
    let manifest;
    try { manifest = JSON.parse(readFileSync(join(root, 'track.json'), 'utf8')); }
    catch { continue; }
    definitions.push({ kind: 'track', source: join(root, 'track.json'), value: manifest });
    const scenarios = join(root, 'scenarios');
    for (const file of readdirSync(scenarios).filter(name => name.endsWith('.json'))) {
      definitions.push({ kind: 'scenario', source: join(scenarios, file),
        value: JSON.parse(readFileSync(join(scenarios, file), 'utf8')) });
    }
  }
  return definitions;
}

test('all current definitions compile deterministically without source mutation', () => {
  const definitions = currentDefinitions();
  assert(definitions.length > 10);
  for (const definition of definitions) {
    const compile = definition.kind === 'track' ? compileTrackManifest : compileScenarioDefinition;
    const original = structuredClone(definition.value);
    const first = compile(definition.value, { source: definition.source });
    const second = compile(definition.value, { source: definition.source });
    assert.equal(first.schemaVersion, DEFINITION_SCHEMA_VERSION);
    assert.deepEqual(first, second);
    assert.deepEqual(definition.value, original, 'compiler must not mutate source');
  }
});

test('legacy suite inheritance is normalized once while schema v1 requires an explicit policy', () => {
  const legacy = { title: 'Example', validatedThrough: 1, plannedThrough: 1,
    portOffset: 500, restartProbe: '/ready', suites: { 1: [
      { id: 'features', spec: 'scenarios/01-features.json' },
      { id: 'invariants', spec: 'scenarios/01-invariants.json' },
    ] } };
  const compiled = compileTrackManifest(legacy);
  assert.equal(compiled.suites['1'][0].inherit, 'none');
  assert.equal(compiled.suites['1'][1].inherit, 'all-higher-levels');
  assert.throws(() => compileTrackManifest({ ...legacy, schemaVersion: 1 }),
    /inherit: is required in schema v1/);
});

test('the scenario language is an explicit 49-action registry', () => {
  assert.equal(ACTION_IDS.length, 49);
  assert.deepEqual(ACTION_REGISTRY.ids, ACTION_IDS);
  assert(ACTION_IDS.includes('clickConcurrently'));
  assert(ACTION_IDS.includes('restartBackend'));
});

test('the extracted executor modules cover every action exactly once', () => {
  const grader = readFileSync(join(TRACKS_DIR, '..', 'grader', 'grade.mjs'), 'utf8');
  const runtime = grader.slice(grader.indexOf('async function runStep('),
    grader.indexOf('// ─── Feature grading'));
  const legacyOccurrences = [
    ...[...runtime.matchAll(/case '([a-zA-Z]+)':/g)].map(match => match[1]),
    ...[...runtime.matchAll(/step\.do === '([a-zA-Z]+)'/g)].map(match => match[1]),
  ];
  const legacy = new Set(legacyOccurrences);
  const extracted = [...BROWSER_ACTION_IDS, ...ACTOR_TRANSPORT_ACTION_IDS,
    ...LIFECYCLE_CONCURRENCY_ACTION_IDS];
  assert.deepEqual([...new Set([...legacy, ...extracted])].sort(), ACTION_IDS);
  assert.deepEqual(extracted.filter(id => legacy.has(id)), []);
  const occurrences = [...legacyOccurrences, ...extracted].reduce((counts, id) =>
    counts.set(id, (counts.get(id) ?? 0) + 1), new Map());
  assert.deepEqual(ACTION_IDS.filter(id => occurrences.get(id) !== 1), []);
});

function scenario(step = { do: 'wait', actor: 'a', ms: 1 }) {
  return { level: 1, features: [{ id: 1, name: 'feature', actors: ['a'], setup: [],
    criteria: [{ id: '1a', desc: 'criterion', steps: [step] }] }] };
}

test('unknown actions and action fields fail before runtime', () => {
  assert.throws(() => compileScenarioDefinition(scenario({ do: 'notReal', actor: 'a' })),
    /unknown action "notReal"/);
  assert.throws(() => compileScenarioDefinition(scenario(
    { do: 'wait', actor: 'a', ms: 1, miliseconds: 1 })), /miliseconds: unknown field/);
});

test('required action fields and nested race steps are validated', () => {
  assert.throws(() => compileScenarioDefinition(scenario({ do: 'wait', actor: 'a' })),
    /\.ms: is required/);
  assert.throws(() => compileScenarioDefinition(scenario({ do: 'race', settleMs: 1,
    branches: [[{ do: 'wait', actor: 'a', ms: 1 }], [{ do: 'wat', actor: 'a', ms: 1 }]] })),
  /branches\[1\]\[0\]\.do: unknown action "wat"/);
});

test('extracted action inputs expose their runtime options without allowing script escapes', () => {
  const compiled = compileScenarioDefinition(scenario({ do: 'forgeWrite', actor: 'a',
    fromActor: 'victim', settleMs: 1, field: 'room', value: 7, text: 'probe' }));
  assert.equal(compiled.features[0].criteria[0].steps[0].value, 7);
  assert.equal(compiled.features[0].criteria[0].points, 1,
    'the compiler must materialize the default before scoring');
  assert.doesNotThrow(() => compileScenarioDefinition(scenario({ do: 'callConcurrently',
    actors: ['a', 'b'], action: 'checkout', settleMs: 1, args: [], body: {} })));
  assert.doesNotThrow(() => compileScenarioDefinition(scenario({ do: 'signUp', actor: 'a',
    name: 'seeded', exact: true })));
  assert.doesNotThrow(() => compileScenarioDefinition(scenario({ do: 'callAction', actor: 'a',
    action: 'buy', input: { testid: 'item-card', contains: 'Desk Lamp',
      attribute: 'data-buy-input' }, authentication: 'none' })));
  assert.throws(() => compileScenarioDefinition(scenario({ do: 'callAction', actor: 'a',
    action: 'buy', input: { testid: 'item-card', attribute: 'data-buy-input' },
    authentication: 'guest' })), /authentication: must be "actor" or "none"/);
  const namedReplay = { do: 'replayAs', actor: 'a', from: 'staff', match: 'ship',
    namedAction: { id: 'ship', path: '/api/fulfilment/ship', reducer: 'ship_order', args: [0] },
    namedTarget: { testid: 'order-item', contains: 'Webcam',
      attribute: 'data-entity-id', valueType: 'number' } };
  assert.doesNotThrow(() => compileScenarioDefinition(scenario(namedReplay)));
  const { namedAction: _omitted, ...missingNamedAction } = namedReplay;
  assert.throws(() => compileScenarioDefinition(scenario(missingNamedAction)),
    /namedAction and namedTarget must be supplied together/);
  assert.throws(() => compileScenarioDefinition(scenario({ ...namedReplay,
    namedTarget: { ...namedReplay.namedTarget, valueType: 'bigint' } })),
  /valueType: must be "number" or "string"/);
  assert.throws(() => compileScenarioDefinition(scenario({ do: 'runScript',
    script: '../outside.mjs', args: [] })), /\.script: has the wrong type or value/);
  assert.throws(() => compileScenarioDefinition(scenario({ do: 'runScript',
    script: 'C:\\outside.mjs', args: [] })), /\.script: has the wrong type or value/);
  assert.throws(() => compileScenarioDefinition(scenario({ do: 'runScript',
    script: 'inside.mjs', args: [], timeoutMs: 90_000 })),
  /\.timeoutMs: has the wrong type or value/);
});

test('declared scenario level is authoritative and can be checked explicitly', () => {
  assert.throws(() => compileScenarioDefinition(scenario(), { expectedLevel: 2 }),
    /declares L1, expected L2/);
});

test('only explicitly withheld zero-point criteria may omit executable steps', () => {
  const value = scenario();
  value.features[0].criteria[0] = { id: '1a', desc: 'future oracle', points: 0,
    withheld: 'awaiting calibration', steps: [] };
  assert.doesNotThrow(() => compileScenarioDefinition(value));
  delete value.features[0].criteria[0].withheld;
  assert.throws(() => compileScenarioDefinition(value), /may be empty only/);
});

test('track manifests reject unknown fields and malformed named actions', () => {
  const base = { title: 'Example', slug: 'example', validatedThrough: 1, plannedThrough: 1,
    portOffset: 500, restartProbe: '/ready', suites: { 1: [{ id: 'features',
      spec: 'scenarios/01.json' }] } };
  assert.throws(() => compileTrackManifest({ ...base, unknowable: true }),
    /unknowable: unknown field/);
  assert.throws(() => compileTrackManifest({ ...base, actions: [{ id: 'buy', path: 'api/buy',
    reducer: 'buy', args: [] }] }), /path: must be an absolute HTTP path/);
  assert.throws(() => compileTrackManifest({ ...base, actions: [{ id: 'buy', path: '/api/buy',
    reducer: 'buy', args: [0], params: [{ name: 'itemId', in: 'path', placeholder: ':id' }] }] }),
  /placeholder: does not appear in path/);
});

test('the live grader rejects malformed definitions before launching a browser', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-definition-preflight-'));
  const spec = join(root, 'invalid.json');
  try {
    writeFileSync(spec, JSON.stringify(scenario(
      { do: 'wait', actor: 'a', ms: 1, miliseconds: 1 })));
    assert.throws(() => execFileSync(process.execPath,
      [join(TRACKS_DIR, '..', 'grader', 'grade.mjs'), '--url', 'http://127.0.0.1:1',
        '--level', '1', '--spec', spec], { encoding: 'utf8', stdio: 'pipe', timeout: 10_000 }),
    error => {
      assert.match(`${error.stdout ?? ''}${error.stderr ?? ''}`, /miliseconds: unknown field/);
      return true;
    });
  } finally { rmSync(root, { recursive: true, force: true }); }
});
