import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import test from 'node:test';

import { resolveGuidanceProfile } from '../src/campaigns/condition-compiler.js';
import type { ResolvedGuidanceProfile } from '../src/campaigns/condition-compiler.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { loadTrack, portsFor } from '../src/composition/tracks.js';
import { resolveBoundRecipeTaskRequest } from '../src/composition/recipe-selection.js';
import { buildPrompt, hostServiceAddress, parseAgentArgs } from '../commands/agent.js';
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
const UNSTATED_QUALITY_LANGUAGE = [
  /\b(?:reload|reconnect|live)\b/i,
  /\b(?:another customer|another account)\b/i,
  /\b(?:changes nothing|without (?:a )?reload)\b/i,
  /\benforce (?:this|the) rule on the server\b/i,
  /\binvalid quantity\b|`-3`/i,
];
type Stack = typeof STACKS[number];
type Level = 1 | 2 | 3 | 4 | 5 | 6;

test('dev SDK ablation removes only the three reference skills', () => {
  const standard = resolveGuidanceProfile('neutral-dev', STACKS);
  const ablation = resolveGuidanceProfile('neutral-dev-no-sdk', STACKS);
  for (const key of ['mode', 'material', 'documents', 'credentialAliases'] as const)
    assert.deepEqual(ablation[key], standard[key]);
  assert.deepEqual(standard.skills.spacetime!.ids,
    ['typescript-server', 'typescript-client', 'cli', 'spacetime-dev']);
  assert.deepEqual(ablation.skills.spacetime!.ids, ['spacetime-dev']);
  for (const stack of ['mongodb', 'postgres']) assert.deepEqual(ablation.skills[stack], standard.skills[stack]);
  const track = loadTrack('ecommerce');
  const catalog = resolveFeatureCatalog('progression/ecommerce.json', track);
  for (const level of [1, 2, 3] as const) {
    const binding = resolveRecipeRelease(track, level, 'ecommerce.progression-catalog');
    const task = resolveProgressionRecipeLevelSelection(binding, catalog, level, { cumulative: true }).agent.request;
    const prompts = [standard, ablation].map(guidance => {
      const skills = readAgentSkillDocuments(resolve(STACK_BENCH_ROOT, '..', '..'), guidance.skills.spacetime!.ids);
      const prompt = renderPrompt({ level, stack: 'spacetime', task, guidance });
      assert(prompt.includes(skills));
      return prompt.replace(skills, '<skill material>');
    });
    assert.equal(prompts[0], prompts[1], `L${level} differs outside the supplied skills`);
  }
});

test('agent contract validation leaves product language unchanged', () => {
  assert.equal(agentVisibleContractText('Use this application action. Keep the contest action.'),
    'Use this application action. Keep the contest action.');
});

function renderPrompt({ level, stack, task, guidance, repair = false, cli = false }: {
  level: Level;
  stack: Stack;
  task: unknown;
  guidance: ResolvedGuidanceProfile;
  repair?: boolean;
  cli?: boolean;
}): string {
  const document = guidance.documents[stack];
  const skills = guidance.skills[stack];
  assert.notEqual(document, undefined);
  assert.ok(skills);
  const argv = [AGENT,
    '--mode', repair ? 'fix' : level === 1 ? 'build' : 'upgrade',
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
  ];
  if (!cli) {
    const args = parseAgentArgs([process.execPath, ...argv]);
    const track = loadTrack('ecommerce');
    const binding = resolveRecipeRelease(track, level, 'ecommerce.progression-catalog');
    const selected = resolveBoundRecipeTaskRequest(binding, args.recipeTask!);
    return buildPrompt(args, portsFor(track, stack, 0), track, {
      skillsText: readAgentSkillDocuments(resolve(STACK_BENCH_ROOT, '..', '..'), skills.ids),
      requirementText: selected.task.requirementText,
      contractText: selected.task.contractText,
      startingCatalog: JSON.stringify({
        warehouses: binding.plan.fixture.warehouses, items: binding.plan.fixture.items,
      }, null, 2),
    });
  }
  return execFileSync(process.execPath, argv, {
    encoding: 'utf8',
    stdio: 'pipe',
    maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, STACK_BENCH_APPLIANCE: '1',
      STACK_BENCH_HOST_ALIAS: hostServiceAddress(),
      STACK_BENCH_IMAGE: 'prompt-review-does-not-use-docker' },
  });
}

test('dev workflow reaches build, upgrade, and repair prompts only when selected', () => {
  const track = loadTrack('ecommerce');
  const catalog = resolveFeatureCatalog('progression/ecommerce.json', track);
  const neutral = resolveGuidanceProfile('neutral', STACKS);
  const dev = resolveGuidanceProfile('neutral-dev', STACKS);
  for (const level of [1, 2] as const) {
    const binding = resolveRecipeRelease(track, level, 'ecommerce.progression-catalog');
    const task = resolveProgressionRecipeLevelSelection(binding, catalog, level,
      { cumulative: true }).agent.request;
    for (const stack of STACKS) for (const repair of [false, true]) {
      const original = renderPrompt({ level, stack, task, guidance: neutral, repair });
      const changed = renderPrompt({ level, stack, task, guidance: dev, repair });
      if (stack === 'spacetime') {
        assert.doesNotMatch(original, /# Development workflow/);
        assert.match(changed, /# Development workflow/);
        assert.match(changed, /Do not run competing publish commands or watchers/);
      } else assert.equal(changed, original);
    }
  }
});

test('all stacks receive the same installed browser client in build, upgrade, and repair prompts', () => {
  const track = loadTrack('ecommerce');
  const catalog = resolveFeatureCatalog('progression/ecommerce.json', track);
  const guidance = resolveGuidanceProfile('neutral', STACKS);
  const paragraphs = new Set<string>();
  for (const level of [1, 2, 3] as const) {
    const binding = resolveRecipeRelease(track, level, 'ecommerce.progression-catalog');
    const task = resolveProgressionRecipeLevelSelection(binding, catalog, level,
      { cumulative: true }).agent.request;
    for (const stack of STACKS) for (const repair of [false, true]) {
      const paragraph = renderPrompt({ level, stack, task, guidance, repair })
        .split('\n').find(line => line.startsWith('Chromium is installed'));
      assert.ok(paragraph);
      assert.match(paragraph, /require\("\/opt\/browser-tools\/node_modules\/puppeteer-core"\)/);
      assert.match(paragraph, /executablePath: process.env.CHROME_BIN/);
      assert.doesNotMatch(paragraph, EVALUATION_LANGUAGE);
      paragraphs.add(paragraph);
    }
  }
  assert.equal(paragraphs.size, 1);
});

test('scheduled restock prompts define names and reducer argument types for every stack', () => {
  const track = loadTrack('ecommerce');
  const catalog = resolveFeatureCatalog('progression/ecommerce.json', track);
  const guidance = resolveGuidanceProfile('neutral-dev', STACKS);
  const binding = resolveRecipeRelease(track, 3, 'ecommerce.progression-catalog');
  const task = resolveProgressionRecipeLevelSelection(binding, catalog, 3,
    { cumulative: true }).agent.request;
  for (const stack of STACKS) for (const repair of [false, true]) {
    const prompt = renderPrompt({ level: 3, stack, task, guidance, repair });
    assert.match(prompt, /`item` and `warehouse` are their names as strings/);
    assert.match(prompt, /`delaySeconds` are JSON integers/);
    if (stack === 'spacetime') {
      assert.match(prompt, /`item: string`, `warehouse: string`/);
      assert.match(prompt, /`quantity: u32`, `delaySeconds: u32`/);
    }
  }
});

test('neutral dependency prompts include only selected product and stack contracts', () => {
  const track = loadTrack('ecommerce');
  const catalog = resolveFeatureCatalog('progression/ecommerce.json', track);
  const guidance = resolveGuidanceProfile('neutral', STACKS);
  const spacetimeReference = readAgentSkillDocuments(
    resolve(STACK_BENCH_ROOT, '..', '..'), guidance.skills.spacetime?.ids ?? []);
  assert.deepEqual(guidance.skills.spacetime?.ids, ['typescript-server', 'typescript-client', 'cli']);
  assert.match(spacetimeReference, /spacetime publish/);
  assert.match(spacetimeReference, /withToken/);
  assert.match(spacetimeReference, /ctx\.sender/);
  for (const level of [1, 2, 3, 4, 5, 6] as const) {
    const binding = resolveRecipeRelease(track, level, 'ecommerce.progression-catalog');
    const selected = resolveProgressionRecipeLevelSelection(binding, catalog, level,
      { cumulative: true });
    assert.deepEqual(selected.agent.request.selection.requested.specifications, {
      requested: [],
      expected: [],
      observed: [],
    });
    assert(selected.grader.request.selection.requested.specifications.expected.length > 0);
    if (level === 2) {
      assert(selected.grader.selection.scoredChecks.some(check =>
        check.stableKey.endsWith('.620a') && check.treatment === 'expected'));
    }
    const moduleTypes = new Map(binding.release.components.packs
      .map(pack => [pack.id, pack.moduleType]));
    for (const check of selected.grader.selection.scoredChecks) {
      assert.equal(check.treatment,
        moduleTypes.get(check.packId ?? '') === 'specification' ? 'expected' : 'requested');
    }
    for (const stack of STACKS) {
      const prompt = renderPrompt({ level, stack, task: selected.agent.request, guidance });
      const repair = renderPrompt({ level, stack, task: selected.agent.request, guidance, repair: true });
      if (level === 2) {
        assert.match(prompt, /Use `profile-address-summary` to display\s+the saved address in the profile view/);
        assert.doesNotMatch(prompt, /in the same session or a new one/);
      }
      // Retain process-boundary coverage for all stacks and all three modes.
      if ((level === 1 && stack === 'mongodb') || (level === 2 && stack === 'postgres')
        || (level === 3 && stack === 'spacetime')) {
        assert.equal(renderPrompt({ level, stack, task: selected.agent.request, guidance,
          repair: level === 3, cli: true }), level === 3 ? repair : prompt);
      }
      assert.doesNotMatch(repair, /Gaming Mouse row uses|`1\.00` as the price/);
      const repairInterface = repair.slice(repair.lastIndexOf('## Application interface'));
      assert.doesNotMatch(repairInterface, EVALUATION_LANGUAGE);
      assert.doesNotMatch(repairInterface, stack === 'spacetime'
        ? /\b(?:GET|POST|PATCH|DELETE|PUT) \// : /\breducer(?:s)?\b/i);
      assert.doesNotMatch(prompt,
        new RegExp(`${EVALUATION_LANGUAGE.source}|Branding & Styling|App title:|<!-- /?interface`, 'i'));
      assert.doesNotMatch(prompt, /\blevel\s+\d+\b/i);
      assert.doesNotMatch(prompt,
        /After the client|client must listen|client architecture|application behavior/i);
      const marker = level === 1 ? '## New application' : '## Existing application';
      const markerIndex = prompt.indexOf(marker);
      assert.notEqual(markerIndex, -1);
      const applicationRequest = prompt.slice(markerIndex);
      assert.doesNotMatch(applicationRequest, /Gaming Mouse row uses|`1\.00` as the price/);
      assert.match(applicationRequest, /## Application interface/);
      // Later product features explicitly request live or account-specific behavior.
      if (level <= 3) for (const language of UNSTATED_QUALITY_LANGUAGE) {
        assert.doesNotMatch(applicationRequest, language);
      }
      assert.doesNotMatch(applicationRequest, /## External data synchronization/);
      const startingCatalog = JSON.stringify({
        warehouses: binding.plan.fixture.warehouses,
        items: binding.plan.fixture.items,
      }, null, 2);
      for (const request of [applicationRequest, repair]) {
        assert.ok(request.includes(startingCatalog));
        assert.equal(request.split('## Starting catalog').length, 2);
      }
      assert.match(repair, /Do not reset current quantities, prices, or user data/);
      if (level > 1) assert.match(applicationRequest, /original catalog baseline/);
      if (stack === 'spacetime') {
        assert.match(prompt, /file:\/deps\/spacetimedb\.tgz/);
        assert.doesNotMatch(prompt, /file:\/deps\/bindings-typescript/);
        assert.doesNotMatch(applicationRequest, /\b(?:GET|POST|PATCH|DELETE) \//);
        if (level === 1) assert.match(applicationRequest, /`signUp` and `signIn` reducers/);
      } else {
        if (level <= 3) assert.match(applicationRequest, /\b(?:GET|POST|PATCH|DELETE) \//);
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
    assert.match(prompt, /APP_WARM_START=1.*reuse them instead of installing them again/s);
    assert.match(prompt, /script must not change source files/);
    assert.doesNotMatch(prompt, /package cache/i);
    assert.doesNotMatch(prompt, /npm `start` script|either `\/app\/start\.sh`/);
    if (stack !== 'spacetime') {
      assert.match(prompt, /service is already running/);
      assert.match(prompt, /Do not\s+start another .* server/);
      assert.match(prompt, /Serve the complete application on `\d+`/);
      assert.doesNotMatch(prompt, /Application service port/);
    } else {
      assert.match(prompt, /spacetime publish/);
      assert.match(prompt, /withToken/);
      assert.match(prompt, /ctx\.sender/);
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
  const identity = resolveGuidanceProfile('neutral', ['spacetime']).skills.spacetime;
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


test('all dependency depths keep production guarantees out of product work and interfaces', () => {
  const track = loadTrack('ecommerce');
  const catalog = resolveFeatureCatalog('progression/ecommerce.json', track);
  const guidance = resolveGuidanceProfile('neutral', STACKS);
  const disclosedGuarantees = /change together|applied only once|does not create a second payment|without a reload|cannot attach or inspect another|another customer's purchase history|cancelled restock never changes stock|restores the stock to its original warehouse|same authorization and price rules|same administrator, stock, and warehouse rules|invalid quantity|`-3`/i;
  for (const level of [1, 2, 3, 4, 5, 6] as const) {
    const binding = resolveRecipeRelease(track, level, 'ecommerce.progression-catalog');
    const task = resolveProgressionRecipeLevelSelection(binding, catalog, level,
      { cumulative: true }).agent.request;
    for (const stack of STACKS) {
      const prompt = renderPrompt({ level, stack, task, guidance });
      const marker = level === 1 ? '## New application' : '## Existing application';
      const product = prompt.slice(prompt.indexOf(marker));
      assert.doesNotMatch(product, disclosedGuarantees, `${stack} depth ${level}`);
      assert.match(product, /## Starting catalog/);
      assert.match(product, /## Application interface/);
    }
  }
});
