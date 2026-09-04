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
  type CompiledStep,
} from '../src/composition/definition-compiler.js';
import { ACTION_IMPLEMENTATIONS, ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { TRACKS_DIR } from '../src/composition/tracks.js';

interface CurrentDefinition {
  kind: 'scenario' | 'track';
  source: string;
  value: unknown;
}

function currentDefinitions(): CurrentDefinition[] {
  const definitions: CurrentDefinition[] = [];
  for (const track of readdirSync(TRACKS_DIR)) {
    const root = join(TRACKS_DIR, track);
    let manifest: unknown;
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
    const original = structuredClone(definition.value);
    const first = definition.kind === 'track'
      ? compileTrackManifest(definition.value, { source: definition.source })
      : compileScenarioDefinition(definition.value, { source: definition.source });
    const second = definition.kind === 'track'
      ? compileTrackManifest(definition.value, { source: definition.source })
      : compileScenarioDefinition(definition.value, { source: definition.source });
    assert.equal(first.schemaVersion, DEFINITION_SCHEMA_VERSION);
    assert.deepEqual(first, second);
    assert.deepEqual(definition.value, original, 'compiler must not mutate source');
  }
});

test('authored waits and observation windows fit inside their action deadlines', () => {
  const visit = (step: CompiledStep, source: string): void => {
    const action = ACTION_REGISTRY.get(step.do);
    assert(action, `${source}: ${step.do} must be registered`);
    const deadline = action.timeoutMs;
    if (step.do === 'wait') {
      assert(typeof step.ms === 'number', `${source}: wait must have a duration`);
      assert(deadline > step.ms, `${source}: ${step.do} ${step.ms}ms exceeds its deadline`);
    }
    if (step.within !== undefined) {
      assert(typeof step.within === 'number', `${source}: within must be a number`);
      assert(deadline > step.within, `${source}: ${step.do} ${step.within}ms exceeds its deadline`);
    }
    if (step.do === 'race') {
      assert(Array.isArray(step.branches), `${source}: race must have branches`);
      for (const branch of step.branches) {
        assert(Array.isArray(branch), `${source}: each race branch must be an array`);
        for (const child of branch) {
          assert(isRecord(child) && typeof child.do === 'string', `${source}: race step must be valid`);
          visit(child, source);
        }
      }
    }
  };
  for (const definition of currentDefinitions().filter(item => item.kind === 'scenario')) {
    const compiled = compileScenarioDefinition(definition.value, { source: definition.source });
    for (const feature of compiled.features) {
      [...feature.setup, ...feature.criteria.flatMap(criterion => criterion.steps)]
        .forEach(step => visit(step, definition.source));
    }
  }
});

test('definitions require schema v1 and explicit suite inheritance', () => {
  const manifest = { title: 'Example', validatedThrough: 1, plannedThrough: 1,
    portOffset: 500, restartProbe: '/ready', suites: { 1: [
      { id: 'features', spec: 'scenarios/01-features.json' },
  ] } };
  assert.throws(() => compileTrackManifest(manifest), /schemaVersion: unsupported version undefined/);
  assert.throws(() => compileTrackManifest({ ...manifest, schemaVersion: 1 }),
    /inherit: must be none or all-higher-levels/);
  const { schemaVersion: _, ...missingScenarioVersion } = scenario() as Record<string, unknown>;
  assert.throws(() => compileScenarioDefinition(missingScenarioVersion, { source: 'scenario.json' }),
    /schemaVersion: unsupported version undefined/);
});

test('the action implementation registry covers every action exactly once', () => {
  assert.deepEqual(Object.keys(ACTION_IMPLEMENTATIONS).sort(), ACTION_IDS);
});

function scenario(step: unknown = { do: 'wait', actor: 'a', ms: 1 }): unknown {
  return { schemaVersion: 1, level: 1, features: [{ id: 1, name: 'feature', actors: ['a'], setup: [],
    criteria: [{ id: '1a', desc: 'criterion', steps: [step] }] }] };
}

test('unknown actions and action fields fail before runtime', () => {
  const staleScenario = scenario() as Record<string, unknown>;
  assert.throws(() => compileScenarioDefinition({ ...staleScenario, status: 'draft' }),
    /status: unknown field/);
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
  const feature = compiled.features[0];
  assert(feature, 'the fixture must compile one feature');
  const criterion = feature.criteria[0];
  assert(criterion, 'the fixture must compile one criterion');
  const step = criterion.steps[0];
  assert(step, 'the fixture must compile one step');
  assert.equal(step.value, 7);
  assert.equal(criterion.points, 1,
    'the compiler must materialize the default before scoring');
  assert.doesNotThrow(() => compileScenarioDefinition(scenario({ do: 'callConcurrently',
    actors: ['a', 'b'], action: 'checkout', settleMs: 1, args: [], body: {} })));
  assert.throws(() => compileScenarioDefinition(scenario({ do: 'callConcurrently',
    actors: ['a', 'a'], action: 'checkout', settleMs: 1 })), /at least two distinct actors/);
  assert.throws(() => compileScenarioDefinition(scenario({ do: 'clickConcurrently',
    actors: ['a', 'a'], testid: 'checkout', settleMs: 1 })), /at least two distinct actors/);
  assert.throws(() => compileScenarioDefinition(scenario({ do: 'replayConcurrently',
    actors: ['a', 'a'], settleMs: 1 })), /at least two distinct actors/);
  assert.throws(() => compileScenarioDefinition(scenario({ do: 'sendConcurrently',
    senders: [{ actor: 'a', prefix: 'one', count: 1 },
      { actor: 'a', prefix: 'two', count: 1 }], delayMs: 1 })),
  /at least two distinct actors/);
  assert.throws(() => compileScenarioDefinition(scenario({ do: 'expectAgreement',
    actors: ['a'], testid: 'total' })), /at least two distinct actors/);
  assert.throws(() => compileScenarioDefinition(scenario({ do: 'wait', actor: 'a', ms: -1 })),
    /wrong type or value/);
  assert.doesNotThrow(() => compileScenarioDefinition(scenario({ do: 'expectNotReceived',
    actor: 'a', contains: 'secret', within: 1000 })));
  assert.doesNotThrow(() => compileScenarioDefinition(scenario({ do: 'signUp', actor: 'a',
    name: 'seeded', exact: true })));
  assert.doesNotThrow(() => compileScenarioDefinition(scenario({ do: 'callAction', actor: 'a',
    action: 'buy', input: { testid: 'item-card', contains: 'Desk Lamp',
      attribute: 'data-buy-input' }, authentication: 'none' })));
  assert.doesNotThrow(() => compileScenarioDefinition(scenario({ do: 'expectActionOutcome',
    actor: 'a', outcome: 'refused', routeProvenBy: 'owner' })));
  assert.throws(() => compileScenarioDefinition(scenario({ do: 'callAction', actor: 'a',
    action: 'buy', input: { testid: 'item-card', attribute: 'data-buy-input' },
    authentication: 'guest' })), /authentication: must be "actor" or "none"/);
  const namedReplay = { do: 'replayAs', actor: 'a', from: 'staff', match: 'ship',
    namedAction: { id: 'ship', path: '/api/fulfilment/ship', reducer: 'ship_order', args: [0],
      params: [{ name: 'orderId', in: 'body', wireType: 'u64' }] },
    namedTarget: { testid: 'order-item', contains: 'Webcam',
      attribute: 'data-entity-id', valueType: 'number' } };
  assert.doesNotThrow(() => compileScenarioDefinition(scenario(namedReplay)));
  const { namedTarget: _target, ...literalNamedReplay } = namedReplay;
  assert.doesNotThrow(() => compileScenarioDefinition(scenario(literalNamedReplay)));
  assert.doesNotThrow(() => compileScenarioDefinition(scenario({ ...namedReplay,
    namedAction: { ...namedReplay.namedAction, method: 'PATCH' } })));
  assert.throws(() => compileScenarioDefinition(scenario({ ...namedReplay,
    namedAction: { ...namedReplay.namedAction, method: 'GET' } })),
  /method: must be "DELETE", "PATCH", "POST", or "PUT"/);
  const { namedAction: _omitted, ...missingNamedAction } = namedReplay;
  assert.throws(() => compileScenarioDefinition(scenario(missingNamedAction)),
    /namedTarget requires namedAction/);
  assert.throws(() => compileScenarioDefinition(scenario({ ...namedReplay,
    namedTarget: { ...namedReplay.namedTarget, valueType: 'bigint' } })),
  /valueType: must be "number" or "string"/);
  assert.throws(() => compileScenarioDefinition(scenario({ ...namedReplay,
    namedAction: { ...namedReplay.namedAction,
      params: [{ name: 'orderId', in: 'body', wireType: 'f64' }] } })),
  /wireType: must be "u64"/);
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
  const withheld = { schemaVersion: 1, level: 1,
    features: [{ id: 1, name: 'feature', actors: ['a'], setup: [],
    criteria: [{ id: '1a', desc: 'future oracle', points: 0,
      withheld: 'awaiting calibration', steps: [] }] }] };
  assert.doesNotThrow(() => compileScenarioDefinition(withheld));
  const missingReason = { schemaVersion: 1, level: 1,
    features: [{ id: 1, name: 'feature', actors: ['a'], setup: [],
    criteria: [{ id: '1a', desc: 'future oracle', points: 0, steps: [] }] }] };
  assert.throws(() => compileScenarioDefinition(missingReason), /may be empty only/);
});

test('track manifests reject unknown fields and malformed named actions', () => {
  const base = { schemaVersion: 1, title: 'Example', slug: 'example', validatedThrough: 1, plannedThrough: 1,
    portOffset: 500, restartProbe: '/ready', suites: { 1: [{ id: 'features',
      inherit: 'none', spec: 'scenarios/01.json' }] } };
  assert.throws(() => compileTrackManifest({ ...base, unknowable: true }),
    /unknowable: unknown field/);
  assert.throws(() => compileTrackManifest({ ...base, actions: [{ id: 'buy', path: 'api/buy',
    reducer: 'buy', args: [] }] }), /path: must be an absolute HTTP path/);
  assert.throws(() => compileTrackManifest({ ...base, actions: [{ id: 'buy', path: '/api/buy',
    reducer: 'buy', args: [0], params: [{ name: 'itemId', in: 'path', placeholder: ':id' }] }] }),
  /placeholder: does not appear in path/);
  const signUp = { id: 'signUp', path: '/api/auth/signup', reducer: 'sign_up', args: ['', ''] };
  const provenance = { action: 'signUp', markerParameter: 'username',
    body: { username: '', password: 'password' } };
  assert.throws(() => compileTrackManifest({ ...base,
    databaseProvenance: { ...provenance, action: 'missing' }, actions: [signUp] }),
  /action: must name one declared action/);
  assert.throws(() => compileTrackManifest({ ...base,
    databaseProvenance: { ...provenance, markerParameter: 'missing' }, actions: [signUp] }),
  /markerParameter: must name one field/);
  assert.doesNotThrow(() => compileTrackManifest({ ...base,
    databaseProvenance: provenance, actions: [signUp] }));
  assert.doesNotThrow(() => compileTrackManifest({ ...base, reseedOnReset: true }));
});

test('the live grader rejects malformed definitions before launching a browser', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-definition-preflight-'));
  const spec = join(root, 'invalid.json');
  try {
    writeFileSync(spec, JSON.stringify(scenario(
      { do: 'wait', actor: 'a', ms: 1, miliseconds: 1 })));
    assert.throws(() => execFileSync(process.execPath,
      [join(TRACKS_DIR, '..', 'dist', 'grader', 'grade.js'), '--url', 'http://127.0.0.1:1',
        '--level', '1', '--spec', spec], { encoding: 'utf8', stdio: 'pipe', timeout: 10_000 }),
    error => {
      assert(isRecord(error), 'the grader must return a process error');
      assert.match(`${String(error.stdout ?? '')}${String(error.stderr ?? '')}`,
        /miliseconds: unknown field/);
      return true;
    });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
