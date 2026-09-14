import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { resolveGuidanceProfile } from '../src/campaigns/condition-compiler.js';
import { agentVisibleContractText, contractInterfaceNames }
  from '../src/composition/agent-visible-contract.js';
import { loadTrack } from '../src/composition/tracks.js';
import type { NamedAction, Track } from '../src/composition/tracks.js';
import { requireRecipeRelease } from '../src/composition/recipe-release.js';
import { resolveBoundRecipeTaskRequest } from '../src/composition/recipe-selection.js';
import type { CompiledStep } from '../src/composition/definition-compiler.js';
import { compileDependencyPolicyInput, dependencyRuntimeDefinition }
  from '../src/progression/progression-definition.js';
import { progressionEngine } from '../src/progression/progression-engine.js';
import { priorProgressionContractIds, resolveProgressionRecipeAction }
  from '../src/progression/progression-recipe-selection.js';

const reference = join(STACK_BENCH_ROOT, 'appliance', 'campaign.ecommerce-progression-reference.json');
const placeholder = (route: string): string => route.replace(/\{[a-zA-Z]+\}|:[a-zA-Z]+/g, '{}');

function* eachStep(steps: readonly CompiledStep[]): Generator<CompiledStep> {
  for (const step of steps) {
    yield step;
    for (const branch of step.branches ?? []) yield* eachStep(branch);
  }
}

// Presence is a disclosure guardrail, not semantic proof. Values used for assertions,
// adversarial inputs, wire types, status semantics and parameter order need review.
// Track actions describe executor requirements; their existence is NOT disclosure.
function missingNames(steps: readonly CompiledStep[], delivered: string,
  applicationInterface: 'http' | 'reducer', track: Track): string[] {
  const missing = new Set<string>();
  const needs = (kind: string, name: string | undefined): void => {
    if (name && !delivered.includes(name)) missing.add(`${kind} ${name}`);
  };
  for (const step of eachStep(steps)) {
    for (const control of [step.testid, step.unlessVisible as string | undefined, step.in?.testid,
      (step.namedTarget as { testid?: string } | undefined)?.testid,
      (step.input as { testid?: string } | undefined)?.testid]) needs('control', control);
    for (const attribute of [(step.input as { attribute?: string } | undefined)?.attribute,
      (step.namedTarget as { attribute?: string } | undefined)?.attribute]) needs('attribute', attribute);
    const named = step.namedAction as NamedAction | undefined;
    const action = named ?? ((step.do === 'callAction' || step.do === 'callConcurrently')
      ? track.actions.find(action => action.id === step.action) : undefined);
    if (action) {
      if (applicationInterface === 'reducer') needs('reducer', action.reducer);
      else if (action.path && !placeholder(delivered).includes(placeholder(action.path))) {
        missing.add(`route ${action.path}`);
      }
      for (const parameter of action.params ?? []) {
        // HTTP path parameters are disclosed by their route placeholder. Their
        // internal mapping name need not be part of the public application API.
        if (applicationInterface !== 'http' || parameter.in !== 'path') needs('parameter', parameter.name);
      }
    } else if (step.do === 'callAction' || step.do === 'callConcurrently') {
      missing.add(`action ${step.action}`);
    }
    if (step.do === 'dbSetStock' && !['item', 'warehouse', 'stock']
      .every(table => delivered.includes(`\`${table}(`))) missing.add('stock data interface');
  }
  return [...missing].sort();
}

test('scored interface names are disclosed by their issued dependency step on each stack', () => {
  const track = loadTrack('ecommerce');
  const campaign = compileCampaignFile(reference);
  const catalog = campaign.featureCatalog;
  assert(catalog);
  const bindings = new Map(campaign.definition.levels.map(level =>
    [level, requireRecipeRelease(track, level, 'ecommerce.progression-catalog')]));
  const binding = bindings.values().next().value!;
  const steps = new Map<string, CompiledStep[]>();
  for (const execution of binding.plan.execution) for (const group of execution.checkGroups) {
    for (const criterion of group.feature.criteria) {
      steps.set(`${group.packId}|${group.checkGroupId}|${criterion.id}`,
        [...group.feature.setup, ...criterion.steps]);
    }
  }
  const byKey = new Map(binding.plan.checks.map(check =>
    [check.stableKey, steps.get(`${check.packId}|${check.checkGroupId}|${check.criterionId}`)]));
  const stacks = campaign.definition.stacks.map(stack => stack.id);
  const guidance = resolveGuidanceProfile('neutral', stacks);
  const missing: string[] = [];
  const checked = new Set<string>();
  // Use the production scheduler and compiler, including simultaneous work. No
  // hand-written dependency traversal or facts from future requests are allowed.
  for (const workSelection of ['feature', 'progressive', 'all-at-once'] as const) {
    const policy = compileDependencyPolicyInput(campaign.definition.repair, catalog,
      { workSelection });
    let state = progressionEngine.initialize(dependencyRuntimeDefinition(catalog, policy));
    for (;;) {
      const current = bindings.get(state.level)!;
      const prior = priorProgressionContractIds(state, bindings, current);
      const selected = resolveProgressionRecipeAction(current, state, prior);
      if (!('agent' in selected)) break;
      const task = resolveBoundRecipeTaskRequest(current, selected.agent.request).task;
      for (const stack of stacks) {
        const applicationInterface = guidance.documents[stack]!.applicationInterface;
        const delivered = agentVisibleContractText(
          `${task.requirementText}\n${task.contractText}`, guidance.credentialAliases, applicationInterface);
        for (const key of selected.grader.checkKeys) {
          const list = byKey.get(key);
          assert(list, `${key} has scenario steps`);
          checked.add(key);
          const needs = missingNames(list, delivered, applicationInterface, track);
          if (needs.length) missing.push(`${workSelection} `
            + `${stack} step=${state.attempts.length + 1} ${key}: ${needs.join(', ')}`);
        }
      }
      const grading = progressionEngine.gradingSelection(state);
      state = progressionEngine.recordResult(state, {
        attemptId: `disclosure-${state.attempts.length + 1}`, outcome: 'conclusive',
        nodes: grading.nodeIds.map(id => ({ id, checks: grading.checks
          .filter(check => check.nodeId === id).map(check => ({ id: check.id, outcome: 'pass' })) })),
      });
    }
  }
  assert.deepEqual([...checked].sort(), [...new Set(catalog.definition.nodes
    .flatMap(node => node.gradingChecks.map(check => check.id)))].sort());
  assert.deepEqual(missing, []);
});

test('later disclosure cannot satisfy an earlier check; only issued contracts can be retained', () => {
  const track = loadTrack('ecommerce');
  const campaign = compileCampaignFile(reference);
  assert(campaign.featureCatalog && campaign.dependencyPolicy);
  const guidance = resolveGuidanceProfile('neutral', campaign.definition.stacks.map(stack => stack.id));
  let state = progressionEngine.initialize(dependencyRuntimeDefinition(
    campaign.featureCatalog, campaign.dependencyPolicy));
  const bindings = new Map(campaign.definition.levels.map(level =>
    [level, requireRecipeRelease(track, level, 'ecommerce.progression-catalog')]));
  const first = resolveProgressionRecipeAction(bindings.get(state.level)!, state);
  assert('agent' in first);
  const grading = progressionEngine.gradingSelection(state);
  state = progressionEngine.recordResult(state, {
    attemptId: 'first-request', outcome: 'conclusive', nodes: grading.nodeIds.map(id => ({
      id, checks: grading.checks.filter(check => check.nodeId === id)
        .map(check => ({ id: check.id, outcome: 'pass' })),
    })),
  });
  const binding = bindings.get(state.level)!;
  const second = resolveProgressionRecipeAction(binding, state);
  const retained = resolveProgressionRecipeAction(binding, state,
    priorProgressionContractIds(state, bindings, binding));
  assert('agent' in second && 'agent' in retained);
  for (const applicationInterface of ['http', 'reducer'] as const) {
    const text = (source: string): string => agentVisibleContractText(source,
      guidance.credentialAliases, applicationInterface);
    const firstText = text(first.agent.task.contractText);
    const secondText = text(second.agent.task.contractText);
    const retainedText = text(retained.agent.task.contractText);
    const laterControl = contractInterfaceNames(secondText).find(name => !firstText.includes(name));
    const earlierControl = contractInterfaceNames(firstText).find(name => !secondText.includes(name));
    assert(laterControl && earlierControl, 'successive requests have distinct interface controls');
    const earlyCheck: CompiledStep[] = [{ do: 'click', testid: laterControl }];
    const earlierCheck: CompiledStep[] = [{ do: 'click', testid: earlierControl }];
    assert.deepEqual(missingNames(earlyCheck, firstText, applicationInterface, track), [`control ${laterControl}`]);
    assert.deepEqual(missingNames(earlyCheck, secondText, applicationInterface, track), []);
    assert.deepEqual(missingNames(earlierCheck, secondText, applicationInterface, track), [`control ${earlierControl}`]);
    assert.deepEqual(missingNames(earlierCheck, retainedText, applicationInterface, track), []);
    // Opt-out removes the earlier contract from the current request, but does
    // not erase its historical disclosure. Later material cannot travel back.
    assert.deepEqual(missingNames(earlierCheck, `${firstText}\n${secondText}`, applicationInterface, track), []);
  }
});
