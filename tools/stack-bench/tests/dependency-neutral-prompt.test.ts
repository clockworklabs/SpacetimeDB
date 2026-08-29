import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import test from 'node:test';

import { resolveGuidanceProfile } from '../src/campaigns/condition-compiler.js';
import type { ResolvedGuidanceProfile } from '../src/campaigns/condition-compiler.js';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { loadTrack } from '../src/composition/tracks.mjs';
import { sha256 } from '../src/evidence/provenance.mjs';
import { resolveFeatureCatalog } from '../src/progression/feature-catalog-selection.js';
import { resolveProgressionRecipeLevelSelection }
  from '../src/progression/progression-recipe-selection.js';
import { agentVisibleContractText } from '../src/composition/agent-visible-contract.mjs';

const AGENT = new URL('../commands/agent.mjs', import.meta.url).pathname
  .replace(/^\/(?:[A-Za-z]:)/, value => value.slice(1));
const STACKS = ['mongodb', 'postgres', 'spacetime'] as const;
type Stack = typeof STACKS[number];
type Level = 1 | 2 | 3;
const EXPECTED = {
  1: {
    mongodb: ['590f69da54ab3904c6c1588bc3e8eb0eb9d4ea5f81f68b8eef2eb522c3dde162', 3659],
    postgres: ['f39008be15c65a49b789fe11261ab48b3e6c9097a85c8a6f7ed1c768238d9119', 3698],
    spacetime: ['532e48c755f4e92f2c39f87337a1ebc501cb40495c5bf28c7b0fcc6855db6c6e', 25443],
  },
  2: {
    mongodb: ['c1ab7a9999da0beb8bfcedfb91ad7f0e5c787b98fef1bb4a1d3412297f0b91e9', 6998],
    postgres: ['70e4fed6907e5a18c7093d3ad55dde4d36c414f5190af8b4104b92cfa05f5781', 7037],
    spacetime: ['5da8dc2ed7e8f4e6e6b3a6b9fcb02757700212b66175c229d1abd162850f664d', 28782],
  },
  3: {
    mongodb: ['582d0bbcead8d443b6d8d4ce55f8952d4722569446fb5032b4c806046f63b519', 9494],
    postgres: ['5766e1a30058b0f97562e4ec9047c343c1b5ecd23f787d085dbd684c07a84d2d', 9533],
    spacetime: ['aaa2831aa10f323c6f1073692180e2327c1f5ecc5b656af70e33f0e568abe51a', 31278],
  },
} satisfies Record<Level, Record<Stack, readonly [string, number]>>;

test('agent contract cleanup changes only complete evaluation phrases', () => {
  assert.equal(agentVisibleContractText('Use this test action. Keep the contest action.'),
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
    '--skills-json', JSON.stringify(skills.ids),
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

test('neutral dependency prompts through L3 are exact, symmetric software requests', () => {
  const track = loadTrack('ecommerce');
  const catalog = resolveFeatureCatalog('ecommerce.questlines@1.1.0', track);
  const guidance = resolveGuidanceProfile('neutral@1.2.0', STACKS);
  for (const level of [1, 2, 3] as const) {
    const binding = resolveRecipeRelease(track, level, 'ecommerce.progression-l1-l3@1.1.0');
    const selected = resolveProgressionRecipeLevelSelection(binding, catalog, level,
      { cumulative: true });
    assert.deepEqual(selected.agent.request.selection.requested.specifications, {
      requested: [], expected: [], observed: [],
    });
    let sharedApplicationRequest = null;
    for (const stack of STACKS) {
      const prompt = renderPrompt({ level, stack, task: selected.agent.request, guidance });
      assert.deepEqual([sha256(prompt), Buffer.byteLength(prompt)], EXPECTED[level][stack]);
      assert.doesNotMatch(prompt,
        /\b(?:benchmark|harness|grader|tests?|testing)\b|stackbench|Stack Bench|Branding & Styling|App title:/i);
      const marker = level === 1 ? '## Ecommerce application' : '## Existing application';
      const markerIndex = prompt.indexOf(marker);
      assert.notEqual(markerIndex, -1);
      const applicationRequest = prompt.slice(markerIndex);
      if (sharedApplicationRequest === null) sharedApplicationRequest = applicationRequest;
      else assert.equal(applicationRequest, sharedApplicationRequest);
    }
  }
});
