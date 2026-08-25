import { cpSync, existsSync } from 'node:fs';
import { join } from 'node:path';

import { readArtifact } from '../evidence/artifacts.mjs';
import { classifyBundle } from '../evidence/outcomes.mjs';
import { hashAppSource, snapshotAppSource } from '../runtime/source-snapshot.mjs';
import { preserveLevelCheckpoint } from '../runtime/source-checkpoint.mjs';
import { progressionEngine } from './progression-engine.mjs';
import { gradeBundleToProgressionResult } from './grade-bundle-result.mjs';
import { resolveProgressionRecipeAction } from './progression-recipe-selection.mjs';
import { writeProgressionState } from './progression-state.mjs';

export function liveProgressionStatus(state, stateArtifact = 'progression-state.json') {
  return {
    stateArtifact,
    phase: state.phase,
    level: state.level,
    attempts: state.attempts.length,
    score: progressionEngine.score(state),
  };
}

export function createLiveProgressionExecution({ progression, owner, statePath, runId,
  outputDir, appDir, track, backend, identities, recipeBindings, getRunArtifact,
  onState } = {}) {
  let state = progressionEngine.initialize(progression.definition);

  const status = () => liveProgressionStatus(state);
  const persist = () => {
    writeProgressionState(statePath, {
      progression,
      owner,
      state,
      id: `${runId}-progression-state`,
    });
    onState(status());
  };
  const bind = level => {
    const binding = recipeBindings.get(state.level);
    if (!binding) throw new Error(`dependency progression has no recipe binding for L${state.level}`);
    const selected = resolveProgressionRecipeAction(binding, state);
    if (selected.action.type !== 'terminal' && selected.action.level !== level) {
      throw new Error(`dependency progression requested L${selected.action.level}, not L${level}`);
    }
    return selected;
  };
  const record = ({ selected, bundle, level, repair }) => {
    if (!selected || selected.action.type === 'terminal') return null;
    const sequence = state.attempts.length + 1;
    const evidenceDirectory = join(outputDir, 'progression',
      `attempt-${String(sequence).padStart(3, '0')}`);
    const gradingDirectory = join(appDir, 'stack-bench');
    if (existsSync(gradingDirectory)) {
      cpSync(gradingDirectory, evidenceDirectory, {
        recursive: true,
        filter: source => !/[\\/]media([\\/]|$)/.test(source),
      });
    }

    let result;
    if (!bundle?.selection || !existsSync(join(evidenceDirectory, 'bundle.json'))) {
      result = {
        attemptId: `${runId}-progression-${sequence}`,
        outcome: 'inconclusive',
        category: 'harness_failure',
        reason: 'grader produced no grade bundle',
      };
    } else {
      const source = hashAppSource(appDir);
      const recipe = selected.grader.request.recipe;
      result = gradeBundleToProgressionResult(
        readArtifact(join(evidenceDirectory, 'bundle.json'), { expectedKind: 'grade_bundle' }),
        selected.action,
        {
          owner,
          runArtifact: getRunArtifact(),
          progressionIdentity: progression.identity,
          selectionSha256: selected.grader.selectionSha256,
          sourceSha256: source.sha256,
          sequence,
          recipeIdentity: {
            id: recipe.id,
            version: recipe.version,
            sha256: recipe.contentSha256,
          },
        },
      );
      if (result.outcome === 'conclusive') {
        snapshotAppSource(appDir, join(outputDir, 'source'));
        preserveLevelCheckpoint({
          appDir,
          outputDir,
          runId,
          identities,
          track,
          backend,
          level,
          repair,
          outcome: classifyBundle(bundle),
          selectionSha256: selected.grader.selectionSha256,
        });
      }
    }
    state = progressionEngine.recordResult(state, result);
    persist();
    return progressionEngine.nextAction(state);
  };

  return {
    initialize: persist,
    bind,
    record,
    get state() { return state; },
    status,
  };
}
