import { cpSync, existsSync, lstatSync, mkdirSync, readdirSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';

import { canonicalDefinitionJson } from '../composition/definition-plan.mjs';
import { currentEngineIdentity, readArtifact } from '../evidence/artifacts.mjs';
import { classifyBundle } from '../evidence/outcomes.mjs';
import { hashDirectory, sha256 } from '../evidence/provenance.mjs';
import { hashAppSource, restoreAppSource, snapshotAppSource }
  from '../runtime/source-snapshot.mjs';
import { preserveLevelCheckpoint } from '../runtime/source-checkpoint.mjs';
import { progressionEngine } from './progression-engine.mjs';
import { replayDependencyMode } from './dependency-mode.mjs';
import { gradeBundleToProgressionResult } from './grade-bundle-result.mjs';
import { resolveProgressionRecipeAction } from './progression-recipe-selection.mjs';
import { progressionStateExists, readProgressionState, validateProgressionOwner,
  writeProgressionState } from './progression-state.js';

function rejectSymlinks(path, label) {
  if (lstatSync(path).isSymbolicLink()) throw new Error(`${label} cannot be a symbolic link`);
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    const child = join(path, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`${label} contains a symbolic link`);
    if (entry.isDirectory()) rejectSymlinks(child, label);
  }
}

function actionSha256(state) {
  return sha256(canonicalDefinitionJson(progressionEngine.nextAction(state)));
}

export function liveProgressionStatus(state, stateArtifact = 'progression-state.json') {
  return {
    stateArtifact,
    phase: state.phase,
    level: state.level,
    attempts: state.attempts.length,
    score: progressionEngine.score(state),
  };
}

export function createLiveProgressionExecution({ progression, featureCatalogIdentity,
  dependencyPolicyIdentity, owner, statePath, runId,
  outputDir, appDir, track, backend, identities, recipeBindings, getRunArtifact,
  resumeFrom = null, onState } = {}) {
  owner = validateProgressionOwner(owner, { requireWorkspace: true });
  const stateRoot = dirname(resolve(statePath));
  if (resolve(outputDir) !== stateRoot) {
    throw new Error('live dependency progression state must be stored in its output directory');
  }
  const sourceDirectory = resolve(stateRoot, owner.workspace.appDirectory);
  const sourceRelative = relative(stateRoot, sourceDirectory);
  if (sourceRelative === '..' || sourceRelative.startsWith(`..${sep}`) || sourceRelative === '') {
    throw new Error('live dependency progression source escapes its output directory');
  }
  let state = null;
  let initialized = false;
  let resumed = false;
  let priorRun = null;
  let resumedSnapshotSha256 = null;

  const status = () => liveProgressionStatus(state);
  const sourceBinding = ({ capture = false } = {}) => {
    if (capture) {
      mkdirSync(appDir, { recursive: true });
      snapshotAppSource(appDir, sourceDirectory);
    }
    if (!existsSync(sourceDirectory)) {
      throw new Error('live dependency progression has no saved source');
    }
    rejectSymlinks(sourceDirectory, 'live dependency progression source');
    const source = hashDirectory(sourceDirectory);
    if (capture) {
      const live = hashAppSource(appDir);
      if (live.sha256 !== source.sha256 || live.files.length !== source.files.length) {
        throw new Error('saved dependency progression source differs from the live application');
      }
    }
    return { actionSha256: actionSha256(state), source: {
      directory: owner.workspace.appDirectory,
      sha256: source.sha256,
      files: source.files.length,
    } };
  };
  const validateSavedSource = (stored, root) => {
    if (!stored.resume
      || stored.resume.actionSha256 !== actionSha256(stored.state)
      || stored.resume.source.directory !== owner.workspace.appDirectory) {
      throw new Error('saved dependency progression action does not match its state');
    }
    const source = resolve(root, stored.resume.source.directory);
    const rel = relative(root, source);
    if (rel === '..' || rel.startsWith(`..${sep}`) || rel === '' || !existsSync(source)) {
      throw new Error('saved dependency progression source is outside its state directory');
    }
    rejectSymlinks(source, 'saved dependency progression source');
    const actual = hashDirectory(source);
    if (actual.sha256 !== stored.resume.source.sha256
      || actual.files.length !== stored.resume.source.files) {
      throw new Error('saved dependency progression source does not match its state');
    }
    return source;
  };
  const validatePriorRun = (root, stored) => {
    const artifact = readArtifact(join(root, 'run.json'), { expectedKind: 'benchmark_run' });
    const runOwner = artifact.payload.progressionOwner;
    const lastEvent = stored.state.events?.at(-1);
    const runState = lastEvent?.type === 'strikes-granted'
      ? replayDependencyMode(stored.state.definition, stored.state.events.slice(0, -1))
      : stored.state;
    if (artifact.attempt.parentId !== owner.attempt.id
      || artifact.identities.engine.sha256 !== currentEngineIdentity().sha256
      || canonicalDefinitionJson(runOwner)
        !== canonicalDefinitionJson({ schemaVersion: 1, campaign: owner.campaign,
          attempt: owner.attempt })
      || canonicalDefinitionJson(artifact.payload.featureCatalog)
        !== canonicalDefinitionJson(featureCatalogIdentity)
      || canonicalDefinitionJson(artifact.payload.dependencyPolicy)
        !== canonicalDefinitionJson(dependencyPolicyIdentity)
      || artifact.payload.backend !== owner.attempt.stack
      || artifact.payload.model !== owner.attempt.model
      || artifact.payload.condition?.sha256 !== owner.attempt.conditionSha256
      || artifact.payload.progressionStatus?.phase !== runState.phase
      || artifact.payload.progressionStatus?.level !== runState.level
      || artifact.payload.progressionStatus?.attempts !== runState.attempts.length) {
      throw new Error('prior dependency execution does not match its saved progression state');
    }
    return artifact;
  };
  const restoreRepairEvidence = (root, targetRoot = root) => {
    const action = progressionEngine.nextAction(state);
    if (action.type !== 'repair') return;
    const index = state.attempts.findLastIndex(attempt => attempt.outcome === 'conclusive');
    const attempt = state.attempts[index];
    if (index < 0 || !attempt?.sourceSha256 || !attempt.selectionSha256
      || attempt.evidence?.kind !== 'grade_bundle') {
      throw new Error('saved repair action has no conclusive grading evidence');
    }
    const from = join(root, 'progression', `attempt-${String(index + 1).padStart(3, '0')}`);
    const bundle = readArtifact(join(from, 'bundle.json'), { expectedKind: 'grade_bundle' });
    if (bundle.id !== attempt.evidence.id
      || sha256(canonicalDefinitionJson(bundle)) !== attempt.evidence.sha256
      || bundle.identities.engine.sha256 !== currentEngineIdentity().sha256
      || bundle.payload.source?.sha256 !== attempt.sourceSha256
      || bundle.payload.selection?.sha256 !== attempt.selectionSha256) {
      throw new Error('saved repair evidence does not match its progression attempt');
    }
    const target = join(targetRoot, 'progression',
      `attempt-${String(index + 1).padStart(3, '0')}`);
    if (resolve(from) !== resolve(target)) {
      cpSync(from, target, { recursive: true,
        filter: source => !/[\\/]media([\\/]|$)/.test(source) });
    }
    const gradingDirectory = join(appDir, 'stack-bench');
    cpSync(target, gradingDirectory, { recursive: true,
      filter: source => !/[\\/]media([\\/]|$)/.test(source) });
  };
  const persist = ({ captureSource = false, notify = true } = {}) => {
    const resume = sourceBinding({ capture: captureSource });
    const written = writeProgressionState(statePath, {
      progression,
      featureCatalogIdentity,
      dependencyPolicyIdentity,
      owner,
      state,
      resume,
      id: `${runId}-progression-state`,
    });
    if (notify && onState) onState(status());
    return written;
  };
  const initialize = () => {
    if (initialized) throw new Error('live dependency progression is already initialized');
    if (progressionStateExists(statePath)) {
      if (resumeFrom !== null) {
        throw new Error('live dependency progression cannot import over existing state');
      }
      const stored = readProgressionState(statePath, { progression, featureCatalogIdentity,
        dependencyPolicyIdentity, owner,
        requireCurrentEngine: true });
      state = stored.state;
      resumedSnapshotSha256 = stored.snapshotSha256;
      validateSavedSource(stored, stateRoot);
      priorRun = validatePriorRun(stateRoot, stored);
      mkdirSync(appDir, { recursive: true });
      restoreAppSource(sourceDirectory, appDir);
      const restored = hashAppSource(appDir);
      if (restored.sha256 !== stored.resume.source.sha256
        || restored.files.length !== stored.resume.source.files) {
        throw new Error('dependency progression source restoration changed its contents');
      }
      restoreRepairEvidence(stateRoot);
      resumed = true;
    } else if (resumeFrom !== null) {
      const previousRoot = resolve(resumeFrom);
      if (previousRoot === stateRoot) {
        throw new Error('dependency progression resume source is the current output directory');
      }
      const stored = readProgressionState(join(previousRoot, 'progression-state.json'), {
        progression, featureCatalogIdentity, dependencyPolicyIdentity, owner,
        requireCurrentEngine: true,
      });
      const previousSource = validateSavedSource(stored, previousRoot);
      state = stored.state;
      resumedSnapshotSha256 = stored.snapshotSha256;
      priorRun = validatePriorRun(previousRoot, stored);
      snapshotAppSource(previousSource, sourceDirectory);
      restoreAppSource(sourceDirectory, appDir);
      const restored = hashAppSource(appDir);
      if (restored.sha256 !== stored.resume.source.sha256
        || restored.files.length !== stored.resume.source.files) {
        throw new Error('imported dependency progression source changed during restoration');
      }
      restoreRepairEvidence(previousRoot, stateRoot);
      persist({ notify: false });
      resumed = true;
    } else {
      state = progressionEngine.initialize(progression.definition);
      persist({ captureSource: true });
    }
    initialized = true;
    return { resumed, action: structuredClone(progressionEngine.nextAction(state)),
      status: status(), snapshotSha256: resumedSnapshotSha256,
      priorRun: priorRun === null ? null : structuredClone(priorRun) };
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
  const record = ({ selected, bundle, level, repair, failure = null }) => {
    if (!selected || selected.action.type === 'terminal') return null;
    const sequence = state.attempts.length + 1;
    const evidenceDirectory = join(outputDir, 'progression',
      `attempt-${String(sequence).padStart(3, '0')}`);
    const gradingDirectory = join(appDir, 'stack-bench');
    if (!failure && existsSync(gradingDirectory)) {
      cpSync(gradingDirectory, evidenceDirectory, {
        recursive: true,
        filter: source => !/[\\/]media([\\/]|$)/.test(source),
      });
    }

    let result;
    if (failure) {
      const category = ['provider_failure', 'harness_failure', 'interrupted', 'inconclusive_evidence']
        .includes(failure.kind) ? failure.kind : 'harness_failure';
      result = {
        attemptId: `${runId}-progression-${sequence}`,
        outcome: 'inconclusive',
        category,
        reason: failure.reason ?? `${category.replaceAll('_', ' ')} during coding`,
      };
    } else if (!bundle?.selection || !existsSync(join(evidenceDirectory, 'bundle.json'))) {
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
          featureCatalogIdentity,
          dependencyPolicyIdentity,
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
    persist({ captureSource: result.outcome === 'conclusive' });
    return progressionEngine.nextAction(state);
  };

  return {
    initialize,
    bind,
    record,
    get state() { return state; },
    get resumed() { return resumed; },
    status,
  };
}
