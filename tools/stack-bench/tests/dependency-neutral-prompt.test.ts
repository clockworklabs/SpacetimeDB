import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import test from 'node:test';

import { resolveGuidanceProfile } from '../src/campaigns/condition-compiler.js';
import type { ResolvedGuidanceProfile } from '../src/campaigns/condition-compiler.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { loadTrack } from '../src/composition/tracks.js';
import { sha256 } from '../src/evidence/provenance.js';
import { resolveFeatureCatalog } from '../src/progression/feature-catalog-selection.js';
import { resolveProgressionRecipeLevelSelection }
  from '../src/progression/progression-recipe-selection.js';
import { agentVisibleContractText } from '../src/composition/agent-visible-contract.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { readAgentSkillDocuments } from '../src/agents/agent-materials.js';

const AGENT = resolve(STACK_BENCH_ROOT, 'dist', 'commands', 'agent.js');
const STACKS = ['mongodb', 'postgres', 'spacetime'] as const;
const EVALUATION_LANGUAGE =
  /\b(?:benchmark|harness|grader|graded|grading|scored|scoring|tests?|testing|evaluation|criterion|testids?)\b|stackbench|Stack Bench|external client|run configuration/i;
type Stack = typeof STACKS[number];
type Level = 1 | 2 | 3;
const EXPECTED = {
  1: {
    mongodb: ['7a68ac41fbf9685904d0de480a889f7b192ffedb697040ac1d256f1a4d48dba7', 5702],
    postgres: ['5444d9d6640f2a350e876ff799b4d7daa42150a375173ddfc6f4f98e8c381c66', 5744],
    spacetime: ['a96468d334a7c9143a1f99a4fe610c28023c63004e8718e6a4afa7da4b935003', 9012],
  },
  2: {
    mongodb: ['ebdd8c887d41274d86fdc65440a123ff35746f7d27b7f9f8ac5b6244eee7eee1', 6661],
    postgres: ['a6b57fce8b373c4a7d2349e252024aad8b899d559c6c8bd7ebe706036d918b90', 6703],
    spacetime: ['f0f6bf0eccb00d7210f8bd00f736e1dcb9d13534213cc8bf0d83d6b97fe7aa4f', 9981],
  },
  3: {
    mongodb: ['99985c31ede49d87b45c402a2f7028978b6486e7439f465b2f7cc30cba02ed37', 9487],
    postgres: ['ab4893755f02adc74339758defee1c0ffa91d66c67b3ee53295d99a6493e5ffd', 9529],
    spacetime: ['3b4d99cc159fa59a16c3925c60081e5d5820dc4ecdfb16540c7eb336752ed1f8', 12740],
  },
} satisfies Record<Level, Record<Stack, readonly [string, number]>>;

test('agent contract validation leaves product language unchanged', () => {
  assert.equal(agentVisibleContractText('Use this application action. Keep the contest action.'),
    'Use this application action. Keep the contest action.');
});

function renderPrompt({ level, stack, task, guidance }: {
  level: Level;
  stack: Stack;
  task: unknown;
  guidance: ResolvedGuidanceProfile;
}): string {
  const document = guidance.documents[stack];
  const skills = guidance.skills[stack];
  assert.notEqual(document, undefined);
  assert.ok(skills);
  return execFileSync(process.execPath, [AGENT,
    '--mode', level === 1 ? 'build' : 'upgrade',
    '--backend', stack,
    '--track', 'ecommerce',
    '--level', String(level),
    '--run-index', '0',
    '--app', '/prompt-review/app',
    '--guidance', 'neutral',
    '--guidance-document-json', JSON.stringify(document),
    '--credential-aliases-json', JSON.stringify(guidance.credentialAliases),
    '--skill-identity-json', JSON.stringify(skills),
    '--recipe-task-json', JSON.stringify(task),
    '--print-prompt',
  ], {
    encoding: 'utf8',
    stdio: 'pipe',
    maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, STACK_BENCH_APPLIANCE: '1',
      STACK_BENCH_IMAGE: 'prompt-review-does-not-use-docker' },
  });
}

test('neutral dependency prompts through L3 contain only the selected stack interface', () => {
  const track = loadTrack('ecommerce');
  const catalog = resolveFeatureCatalog('ecommerce.questlines@2.0.1', track);
  const guidance = resolveGuidanceProfile('neutral@1.7.0', STACKS);
  const spacetimeReference = readAgentSkillDocuments(
    resolve(STACK_BENCH_ROOT, '..', '..'), guidance.skills.spacetime?.ids ?? []);
  assert.match(spacetimeReference, /schema\(\{ record \}\).*spacetimedb\.reducer/s);
  assert.match(spacetimeReference, /DbConnection\.builder\(\).*subscriptionBuilder/s);
  assert.doesNotMatch(spacetimeReference,
    /localStorage|withToken|clientVisibilityFilter|scheduled\s*[:(]|spacetimedb\.(?:view|procedure|httpHandler)/);
  for (const level of [1, 2, 3] as const) {
    const binding = resolveRecipeRelease(track, level, 'ecommerce.progression-depth3@2.0.1');
    const selected = resolveProgressionRecipeLevelSelection(binding, catalog, level,
      { cumulative: true });
    assert.deepEqual(selected.agent.request.selection.requested.specifications, {
      requested: [], expected: [], observed: [],
    });
    for (const stack of STACKS) {
      const prompt = renderPrompt({ level, stack, task: selected.agent.request, guidance });
      assert.deepEqual([sha256(prompt), Buffer.byteLength(prompt)], EXPECTED[level][stack]);
      assert.doesNotMatch(prompt,
        new RegExp(`${EVALUATION_LANGUAGE.source}|Branding & Styling|App title:|<!-- /?interface`, 'i'));
      assert.doesNotMatch(prompt, /\blevel\s+\d+\b/i);
      assert.doesNotMatch(prompt,
        /After the client|client must listen|client architecture|application behavior/i);
      const marker = level === 1 ? '## New application' : '## Existing application';
      const markerIndex = prompt.indexOf(marker);
      assert.notEqual(markerIndex, -1);
      const applicationRequest = prompt.slice(markerIndex);
      assert.match(applicationRequest, /## Application interface/);
      const startingCatalog = JSON.stringify({
        warehouses: binding.plan.fixture.warehouses,
        items: binding.plan.fixture.items,
      }, null, 2);
      if (level === 1) {
        assert.match(applicationRequest, /## Starting catalog/);
        assert.ok(applicationRequest.includes(startingCatalog));
      } else {
        assert.doesNotMatch(applicationRequest, /## Starting catalog/);
      }
      if (stack === 'spacetime') {
        assert.match(prompt, /file:\/deps\/spacetimedb\.tgz/);
        assert.doesNotMatch(prompt, /file:\/deps\/bindings-typescript/);
        assert.doesNotMatch(applicationRequest, /\b(?:GET|POST|PATCH|DELETE) \//);
        if (level === 1) assert.match(applicationRequest, /`signUp` and `signIn` reducers/);
      } else {
        assert.match(applicationRequest, /\b(?:GET|POST|PATCH|DELETE) \//);
        assert.doesNotMatch(applicationRequest, /\breducer(?:s)?\b/i);
        if (level === 1) assert.match(applicationRequest, /POST \/api\/auth\/signup/);
      }
    }
  }
});

test('direct neutral guidance uses the current stack access documents', () => {
  for (const stack of STACKS) {
    const prompt = execFileSync(process.execPath, [AGENT,
      '--mode', 'build',
      '--backend', stack,
      '--track', 'ecommerce',
      '--level', '1',
      '--run-index', '0',
      '--app', '/prompt-review/app',
      '--guidance', 'neutral',
      '--print-prompt',
    ], {
      encoding: 'utf8',
      stdio: 'pipe',
      maxBuffer: 64 * 1024 * 1024,
      env: { ...process.env, STACK_BENCH_APPLIANCE: '1',
        STACK_BENCH_IMAGE: 'prompt-review-does-not-use-docker' },
    });
    assert.doesNotMatch(prompt, /Branding & Styling|App title:/i);
    assert.doesNotMatch(prompt, EVALUATION_LANGUAGE);
    assert.match(prompt, /store-admin-2026/);
    assert.match(prompt, /Create `\/app\/start\.sh`/);
    assert.match(prompt, /clean\s+source checkout.*install\s+dependencies.*build.*start/s);
    assert.match(prompt, /script must not change source files/);
    assert.doesNotMatch(prompt, /package cache/i);
    assert.doesNotMatch(prompt, /npm `start` script|either `\/app\/start\.sh`/);
    if (stack !== 'spacetime') {
      assert.match(prompt, /service is already running/);
      assert.match(prompt, /Do not\s+start another .* server/);
      assert.match(prompt, /Serve the complete application on `\d+`/);
      assert.doesNotMatch(prompt, /Application service port/);
    } else {
      assert.doesNotMatch(prompt,
        /localStorage|withToken|clientVisibilityFilter|scheduled\s*[:(]|spacetimedb\.(?:view|procedure|httpHandler)/);
    }
  }
});

test('direct prescribed SpacetimeDB guidance includes token-handling guidance', () => {
  const prompt = execFileSync(process.execPath, [AGENT,
    '--mode', 'build',
    '--backend', 'spacetime',
    '--track', 'ecommerce',
    '--level', '1',
    '--run-index', '0',
    '--app', '/prompt-review/app',
    '--guidance', 'prescribed',
    '--print-prompt',
  ], {
    encoding: 'utf8',
    stdio: 'pipe',
    maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, STACK_BENCH_APPLIANCE: '1',
      STACK_BENCH_IMAGE: 'prompt-review-does-not-use-docker' },
  });
  assert.match(prompt, /withToken/);
  assert.match(prompt, /localStorage/);
  assert.doesNotMatch(prompt, EVALUATION_LANGUAGE);
  assert.match(prompt, /store-admin-2026/);
});

test('campaign skill material cannot change after compilation', () => {
  const identity = resolveGuidanceProfile('neutral@1.7.0', ['spacetime']).skills.spacetime;
  assert(identity);
  assert.throws(() => execFileSync(process.execPath, [AGENT,
    '--mode', 'build',
    '--backend', 'spacetime',
    '--track', 'ecommerce',
    '--level', '1',
    '--run-index', '0',
    '--app', '/prompt-review/app',
    '--guidance', 'neutral',
    '--skill-identity-json', JSON.stringify({ ...identity, bytes: identity.bytes + 1 }),
    '--print-prompt',
  ], {
    encoding: 'utf8',
    stdio: 'pipe',
    env: { ...process.env, STACK_BENCH_APPLIANCE: '1',
      STACK_BENCH_IMAGE: 'prompt-review-does-not-use-docker' },
  }), /campaign skill material changed after compilation/);
});
