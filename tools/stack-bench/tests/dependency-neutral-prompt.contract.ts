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
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { readAgentSkillDocuments } from '../src/agents/agent-materials.js';

test('production framing is one sentence and never changes restoration prompts', () => {
  const sentence = 'Build a production-quality application suitable for real users, not a prototype or demo.';
  const track = loadTrack('ecommerce');
  for (const backend of ['mongodb', 'postgres', 'spacetime', 'convex']) for (const level of [1, 2, 3, 4, 5, 6])
    for (const mode of ['build', 'upgrade', 'fix', 'resume']) {
    const argv = ['node', 'agent', '--mode', mode, '--backend', backend, '--level', String(level), '--app', '/app', '--guidance', 'neutral'];
    const enabled = parseAgentArgs(argv), disabled = parseAgentArgs([...argv, '--no-production-quality']);
    const materials = { skillsText: '', requirementText: 'Build the requested store.', contractText: '' };
    const render = (args: typeof enabled) => buildPrompt(args, portsFor(track, backend, 0), track, materials);
    const on = render(enabled), off = render(disabled);
    assert.equal(on.includes(sentence), mode !== 'resume');
    assert.equal(on.replace(sentence + '\n\n', ''), off);
  }
});

const AGENT = resolve(STACK_BENCH_ROOT, 'dist', 'commands', 'agent.js');
const STACKS = ['mongodb', 'postgres', 'spacetime', 'convex'] as const;
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

test('SDK skills and dev guidance vary independently without changing product requests', () => {
  const profiles = ['neutral', 'neutral-no-sdk', 'neutral-dev', 'neutral-dev-no-sdk']
    .map(id => resolveGuidanceProfile(id, STACKS));
  const standard = profiles[0]!;
  for (const profile of profiles) {
    for (const key of ['mode', 'material', 'documents', 'credentialAliases'] as const)
      assert.deepEqual(profile[key], standard[key]);
    for (const stack of ['mongodb', 'postgres', 'convex']) assert.deepEqual(profile.skills[stack], standard.skills[stack]);
  }
  assert.deepEqual(profiles.map(p => p.skills.spacetime!.ids), [
    ['typescript-server', 'typescript-client', 'cli'], [],
    ['typescript-server', 'typescript-client', 'cli', 'spacetime-dev'], ['spacetime-dev'],
  ]);
  const track = loadTrack('ecommerce');
  const catalog = resolveFeatureCatalog('progression/ecommerce.json', track);
  for (const level of [1, 2, 3, 4, 5, 6] as const) {
    const binding = resolveRecipeRelease(track, level, 'ecommerce.progression-catalog');
    const task = resolveProgressionRecipeLevelSelection(binding, catalog, level, { cumulative: true }).agent.request;
    for (const stack of STACKS) for (const repair of [false, true]) {
      const prompts = profiles.map(guidance => {
        const skills = readAgentSkillDocuments(STACK_BENCH_ROOT, guidance.skills[stack]!.ids);
        const prompt = renderPrompt({ level, stack, task, guidance, repair });
        assert(prompt.includes(skills));
        return skills ? prompt.replace('\n\n## Selected API reference\n\n' + skills, '') : prompt;
      });
      for (const prompt of prompts) {
        assert.equal(prompt, prompts[0], `${stack} L${level}${repair ? ' repair' : ''} differs outside the supplied skills`);
      }
    }
  }
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
      skillsText: readAgentSkillDocuments(STACK_BENCH_ROOT, skills.ids),
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

test('neutral dependency prompts include only selected product and stack contracts', () => {
  const track = loadTrack('ecommerce');
  const catalog = resolveFeatureCatalog('progression/ecommerce.json', track);
  const guidance = resolveGuidanceProfile('neutral', STACKS);
  const spacetimeReference = readAgentSkillDocuments(
    STACK_BENCH_ROOT, guidance.skills.spacetime?.ids ?? []);
  assert.deepEqual(guidance.skills.spacetime?.ids, ['typescript-server', 'typescript-client', 'cli']);
  assert.match(spacetimeReference, /spacetime publish/);
  assert.match(spacetimeReference, /withToken/);
  assert.match(spacetimeReference, /ctx\.sender/);
  const disclosedGuarantees = /change together|applied only once|does not create a second payment|without a reload|cannot attach or inspect another|another customer's purchase history|cancelled restock never changes stock|restores the stock to its original warehouse|same authorization and price rules|same administrator, stock, and warehouse rules|invalid quantity|`-3`/i;
  const browserClients = new Set<string>();
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
      for (const request of [prompt, repair]) {
        const browserClient = request.split('\n').find(line => line.startsWith('Chromium is installed'));
        assert.ok(browserClient);
        browserClients.add(browserClient);
        if (level === 3) {
          assert.match(request, /`delaySeconds`/);
          if (stack === 'spacetime') {
            assert.match(request, /`item: string`, `warehouse: string`/);
            assert.match(request, /`quantity: u32`, `delaySeconds: u32`/);
          }
        }
        assert.doesNotMatch(request, /an interrupted checkout leaves|earlier orders remain recorded correctly/);
        assert.doesNotMatch(request, /__stackBenchScriptCanary|Stored review marker|stored-review-script/);
        assert.match(request, /Startup must work with an empty database by creating the supplied starting data and accounts/);
        assert.match(request, /On an existing database, preserve current quantities, prices, and user data/);
        assert.match(request, /This applies after upgrades and repairs too/);
        // Direct level requests disclose it with purchasing/checkout. Campaigns retain prior contracts separately.
        assert.equal(request.split('# Order data interface').length - 1, level === 2 || level === 3 ? 1 : 0,
          `${stack} L${level} order data contract`);
        if (level === 2 || level === 3) {
          assert.match(request, /database-native views over the\s+application's current records are allowed/);
          assert.match(request, /When carts are available/);
          assert.match(request, /order_reservation\(account_id, item_id, warehouse_id, quantity\)/);
          assert.match(request, /This does not require adding stock reservations to the app/);
          assert.match(request, /When warehouse stock is available/);
        }
      }
      if (level === 2) {
        assert.match(prompt, /Use `profile-address-summary` to display\s+the saved address in the profile view/);
        assert.doesNotMatch(prompt, /in the same session or a new one/);
      }
      // Retain process-boundary coverage for all stacks and all three modes.
      if ((level === 1 && (stack === 'mongodb' || stack === 'convex')) || (level === 2 && stack === 'postgres')
        || (level === 3 && stack === 'spacetime')) {
        assert.equal(renderPrompt({ level, stack, task: selected.agent.request, guidance,
          repair: level === 3, cli: true }), level === 3 ? repair : prompt);
      }
      assert.doesNotMatch(repair, /Gaming Mouse row uses|`1\.00` as the price/);
      const repairInterface = repair.slice(repair.lastIndexOf('## Application interface'));
      assert.doesNotMatch(repairInterface, EVALUATION_LANGUAGE);
      assert.doesNotMatch(repairInterface, stack === 'spacetime'
        ? /\b(?:GET|POST|PATCH|DELETE|PUT) \// : /\breducer(?:s)?\b/i);
      if (stack === 'convex') {
        assert.doesNotMatch(repairInterface, /\b(?:GET|POST|PATCH|DELETE|PUT) \//);
        assert.doesNotMatch(prompt, /<CONVEX|<EXPRESS_PORT>|<VITE_PORT>/);
      }
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
      assert.doesNotMatch(applicationRequest, disclosedGuarantees, `${stack} depth ${level}`);
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
        if (level === 1) {
          assert.match(applicationRequest, /application's real authentication path/);
          assert.doesNotMatch(applicationRequest, /`signUp` and `signIn` reducers/);
        }
      } else {
        // Account-only L1 permits hosted login; later product operations have their stack's native interface.
        if (level >= 2 && level <= 3) assert.match(applicationRequest, stack === 'convex'
          ? /api:[a-z_]+/ : /\b(?:GET|POST|PATCH|DELETE) \//);
        assert.doesNotMatch(applicationRequest, /\breducer(?:s)?\b/i);
        if (level === 1) {
          assert.match(applicationRequest, /application's real authentication path/);
          assert.doesNotMatch(applicationRequest, /POST \/api\/auth\/(?:signup|signin)/);
        }
      }
    }
  }
  assert.equal(browserClients.size, 1);
  assert.doesNotMatch([...browserClients][0]!, EVALUATION_LANGUAGE);
});

test('direct neutral guidance uses the current stack access documents', () => {
  const cases = [...STACKS.map(stack => ['neutral', stack] as const), ['prescribed', 'spacetime'] as const];
  for (const [guidance, stack] of cases) {
    const prompt = execFileSync(process.execPath, [AGENT,
      '--mode', 'build',
      '--backend', stack,
      '--track', 'ecommerce',
      '--level', '1',
      '--run-index', '0',
      '--app', '/prompt-review/app',
      '--guidance', guidance,
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
    if (stack !== 'spacetime') {
      assert.match(prompt, /service is already running/);
      assert.match(prompt, /Do not\s+start another .* server/);
      assert.match(prompt, /Serve the complete application on `\d+`/);
      assert.doesNotMatch(prompt, /Application service port/);
    } else {
      assert.match(prompt, /withToken/);
      if (guidance === 'prescribed') assert.match(prompt, /localStorage/);
      else {
        assert.match(prompt, /spacetime publish/);
        assert.match(prompt, /ctx\.sender/);
      }
    }
  }
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
