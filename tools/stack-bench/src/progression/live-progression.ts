import { cpSync, existsSync, lstatSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';

import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import type { RecipeBinding } from '../composition/recipe-release.js';
import { ARTIFACT_FILE, artifactPayload, currentEngineIdentity, readArtifact } from '../evidence/artifacts.js';
import type { Artifact } from '../evidence/artifacts.js';
import type { GradeBundlePayload, RunLevelRecord, RunTotals }
  from '../evidence/benchmark-run.js';
import { classifyBundle } from '../evidence/outcomes.js';
import { hashDirectory, sha256 } from '../evidence/provenance.js';
import { hashAppSource, restoreAppSource, snapshotAppSource }
  from '../runtime/source-snapshot.js';
import { preserveLevelCheckpoint } from '../runtime/source-checkpoint.js';
import { replayDependencyMode } from './dependency-mode.js';
import type { DependencyScore } from './dependency-score.js';
import { gradeBundleToProgressionResult } from './grade-bundle-result.js';
import type { ProgressionGradeResult } from './grade-bundle-result.js';
import { progressionEngine } from './progression-engine.js';
import type { ProgressionAction } from './progression-engine.js';
import { validateProgressionInput } from './progression-definition.js';
import type { ProgressionInput } from './progression-definition.js';
import { resolveProgressionRecipeAction } from './progression-recipe-selection.js';
import type { ProgressionRecipeAction } from './progression-recipe-selection.js';
import {
  progressionStateExists,
  readProgressionState,
  validateProgressionOwner,
  writeProgressionState,
} from './progression-state.js';
import type {
  ProgressionOwner,
  ProgressionRepairRegression,
  ProgressionResumeBinding,
  ProgressionState,
} from './progression-state.js';

interface WorkspaceProgressionOwner extends ProgressionOwner {
  workspace: { appDirectory: string };
}

interface BenchmarkRunPayload extends Record<string, unknown> {
  progressionOwner?: unknown;
  featureCatalog?: unknown;
  dependencyPolicy?: unknown;
  backend?: string;
  model?: string;
  condition?: { sha256?: string };
  progressionStatus?: { phase?: string; level?: number; attempts?: number };
  levels?: RunLevelRecord[];
  totals?: RunTotals;
}

interface CodingFailureResult {
  attemptId: string;
  outcome: 'inconclusive';
  category: 'provider_failure' | 'harness_failure' | 'interrupted' | 'inconclusive_evidence';
  reason: string;
}

interface CodingFailure {
  kind?: string;
  reason?: string;
}

interface RecordOptions {
  selected: ProgressionRecipeAction | null;
  bundle: unknown;
  level: number;
  repair: unknown;
  failure?: CodingFailure | null;
  repairRegression?: ProgressionRepairRegression | null;
}

export interface LiveProgressionStatus {
  stateArtifact: string;
  phase: ProgressionState['phase'];
  level: number;
  attempts: number;
  score: DependencyScore;
}

export interface LiveProgressionExecutionOptions {
  progression: ProgressionInput;
  featureCatalogIdentity: unknown;
  dependencyPolicyIdentity: unknown;
  owner: unknown;
  statePath: string;
  runId: string;
  outputDir: string;
  appDir: string;
  track: string;
  backend: string;
  identities: unknown;
  recipeBindings: Map<number, RecipeBinding>;
  getRunArtifact: () => unknown;
  resumeFrom?: string | null;
  onState?: (status: LiveProgressionStatus) => void;
}

export interface LiveProgressionExecution {
  initialize(): {
    resumed: boolean;
    action: ProgressionAction;
    status: LiveProgressionStatus;
    stateSha256: string | null;
    priorRun: Artifact<BenchmarkRunPayload> | null;
  };
  bind(): ProgressionRecipeAction;
  record(options: RecordOptions): ProgressionAction | null;
  readonly state: ProgressionState | null;
  readonly resumed: boolean;
  status(): LiveProgressionStatus;
}

type StoredProgression = ReturnType<typeof readProgressionState>;

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const FAILURE_CATEGORIES = [
  'provider_failure', 'harness_failure', 'interrupted', 'inconclusive_evidence',
] as const satisfies ReadonlyArray<CodingFailureResult['category']>;

function failureCategory(value: unknown): value is CodingFailureResult['category'] {
  return typeof value === 'string'
    && FAILURE_CATEGORIES.some(category => category === value);
}

function rejectSymlinks(path: string, label: string): void {
  if (lstatSync(path).isSymbolicLink()) throw new Error(`${label} cannot be a symbolic link`);
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    const child = join(path, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`${label} contains a symbolic link`);
    if (entry.isDirectory()) rejectSymlinks(child, label);
  }
}

function workspaceOwner(input: unknown): WorkspaceProgressionOwner {
  const owner = validateProgressionOwner(input, { requireWorkspace: true });
  if (!owner.workspace) throw new Error('progression state owner is incomplete');
  return { ...owner, workspace: owner.workspace };
}

function resumeBinding(input: unknown): ProgressionResumeBinding {
  if (!object(input) || typeof input.actionSha256 !== 'string' || !object(input.source)
    || typeof input.source.directory !== 'string' || typeof input.source.sha256 !== 'string'
    || !Number.isSafeInteger(input.source.files) || Number(input.source.files) < 0) {
    throw new Error('saved dependency progression action does not match its state');
  }
  return {
    actionSha256: input.actionSha256,
    source: {
      directory: input.source.directory,
      sha256: input.source.sha256,
      files: Number(input.source.files),
    },
  };
}

function actionSha256(state: ProgressionState): string {
  return sha256(canonicalDefinitionJson(progressionEngine.nextAction(state)));
}

export function liveProgressionStatus(state: ProgressionState,
  stateArtifact = ARTIFACT_FILE.progressionState): LiveProgressionStatus {
  return {
    stateArtifact,
    phase: state.phase,
    level: state.level,
    attempts: state.attempts.length,
    score: progressionEngine.score(state),
  };
}

export function createLiveProgressionExecution(
  options: LiveProgressionExecutionOptions,
): LiveProgressionExecution {
  const progression = validateProgressionInput(options.progression);
  const featureCatalogIdentity = options.featureCatalogIdentity;
  const dependencyPolicyIdentity = options.dependencyPolicyIdentity;
  const owner = workspaceOwner(options.owner);
  const { statePath, runId, outputDir, appDir, track, backend, identities,
    recipeBindings, getRunArtifact, onState } = options;
  const resumeFrom = options.resumeFrom ?? null;
  const stateRoot = dirname(resolve(statePath));
  if (resolve(outputDir) !== stateRoot) {
    throw new Error('live dependency progression state must be stored in its output directory');
  }
  const sourceDirectory = resolve(stateRoot, owner.workspace.appDirectory);
  const sourceRelative = relative(stateRoot, sourceDirectory);
  if (sourceRelative === '..' || sourceRelative.startsWith(`..${sep}`) || sourceRelative === '') {
    throw new Error('live dependency progression source escapes its output directory');
  }
  let state: ProgressionState | null = null;
  let initialized = false;
  let resumed = false;
  let priorRun: Artifact<BenchmarkRunPayload> | null = null;
  let resumedStateSha256: string | null = null;

  const currentState = (): ProgressionState => {
    if (!state) throw new Error('live dependency progression is not initialized');
    return state;
  };
  const status = (): LiveProgressionStatus => liveProgressionStatus(currentState());
  const sourceBinding = ({ capture = false }: { capture?: boolean } = {}): ProgressionResumeBinding => {
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
    return { actionSha256: actionSha256(currentState()), source: {
      directory: owner.workspace.appDirectory,
      sha256: source.sha256,
      files: source.files.length,
    } };
  };
  const validateSavedSource = (stored: StoredProgression, root: string): {
    path: string;
    resume: ProgressionResumeBinding;
  } => {
    const resume = resumeBinding(stored.resume);
    if (resume.actionSha256 !== actionSha256(stored.state)
      || resume.source.directory !== owner.workspace.appDirectory) {
      throw new Error('saved dependency progression action does not match its state');
    }
    const source = resolve(root, resume.source.directory);
    const rel = relative(root, source);
    if (rel === '..' || rel.startsWith(`..${sep}`) || rel === '' || !existsSync(source)) {
      throw new Error('saved dependency progression source is outside its state directory');
    }
    rejectSymlinks(source, 'saved dependency progression source');
    const actual = hashDirectory(source);
    if (actual.sha256 !== resume.source.sha256 || actual.files.length !== resume.source.files) {
      throw new Error('saved dependency progression source does not match its state');
    }
    return { path: source, resume };
  };
  const validatePriorRun = (root: string, stored: StoredProgression):
  Artifact<BenchmarkRunPayload> => {
    const artifact = readArtifact<BenchmarkRunPayload>(join(root, ARTIFACT_FILE.run),
      { expectedKind: 'benchmark_run' });
    const runOwner = artifact.payload.progressionOwner;
    const lastEvent = stored.state.events.at(-1);
    const runState = lastEvent?.type === 'strikes-granted'
      ? replayDependencyMode(stored.state.definition, stored.state.events.slice(0, -1))
      : stored.state;
    const engine = artifact.identities.engine;
    if (artifact.attempt.parentId !== owner.attempt.id
      || engine?.sha256 !== currentEngineIdentity().sha256
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
  const restoreRepairEvidence = (root: string, targetRoot = root): void => {
    const activeState = currentState();
    const action = progressionEngine.nextAction(activeState);
    if (action.type !== 'repair') return;
    const index = activeState.attempts.findLastIndex(attempt => attempt.outcome === 'conclusive');
    const attempt = activeState.attempts[index];
    const evidence = object(attempt?.evidence) ? attempt.evidence : null;
    if (index < 0 || !attempt?.sourceSha256 || !attempt.selectionSha256
      || evidence?.kind !== 'grade_bundle' || typeof evidence.id !== 'string'
      || typeof evidence.sha256 !== 'string') {
      throw new Error('saved repair action has no conclusive grading evidence');
    }
    const from = join(root, 'progression', `attempt-${String(index + 1).padStart(3, '0')}`);
    const bundle = readArtifact<GradeBundlePayload>(join(from, ARTIFACT_FILE.gradeBundle),
      { expectedKind: 'grade_bundle' });
    const engine = bundle.identities.engine;
    if (bundle.id !== evidence.id
      || sha256(canonicalDefinitionJson(bundle)) !== evidence.sha256
      || engine?.sha256 !== currentEngineIdentity().sha256
      || bundle.payload.source?.sha256 !== attempt.sourceSha256
      || bundle.payload.selection?.sha256 !== attempt.selectionSha256) {
      throw new Error('saved repair evidence does not match its progression attempt');
    }
    const target = join(targetRoot, 'progression',
      `attempt-${String(index + 1).padStart(3, '0')}`);
    if (resolve(from) !== resolve(target)) {
      rmSync(target, { recursive: true, force: true });
      cpSync(from, target, { recursive: true,
        filter: source => !/[\\/]media([\\/]|$)/.test(source) });
    }
    const gradingDirectory = join(appDir, 'stack-bench');
    rmSync(gradingDirectory, { recursive: true, force: true });
    cpSync(target, gradingDirectory, { recursive: true,
      filter: source => !/[\\/]media([\\/]|$)/.test(source) });
  };
  const persist = ({ captureSource = false, notify = true }:
  { captureSource?: boolean; notify?: boolean } = {}): StoredProgression => {
    const resume = sourceBinding({ capture: captureSource });
    const written = writeProgressionState(statePath, {
      progression,
      featureCatalogIdentity,
      dependencyPolicyIdentity,
      owner,
      state: currentState(),
      resume,
      id: `${runId}-progression-state`,
    });
    if (notify && onState) onState(status());
    return written;
  };
  const initialize = (): ReturnType<LiveProgressionExecution['initialize']> => {
    if (initialized) throw new Error('live dependency progression is already initialized');
    if (progressionStateExists(statePath)) {
      if (resumeFrom !== null) {
        throw new Error('live dependency progression cannot import over existing state');
      }
      const stored = readProgressionState(statePath, { progression, featureCatalogIdentity,
        dependencyPolicyIdentity, owner, requireCurrentEngine: true });
      state = stored.state;
      resumedStateSha256 = stored.stateSha256;
      const saved = validateSavedSource(stored, stateRoot);
      priorRun = validatePriorRun(stateRoot, stored);
      mkdirSync(appDir, { recursive: true });
      restoreAppSource(sourceDirectory, appDir);
      const restored = hashAppSource(appDir);
      if (restored.sha256 !== saved.resume.source.sha256
        || restored.files.length !== saved.resume.source.files) {
        throw new Error('dependency progression source restoration changed its contents');
      }
      restoreRepairEvidence(stateRoot);
      resumed = true;
    } else if (resumeFrom !== null) {
      const previousRoot = resolve(resumeFrom);
      if (previousRoot === stateRoot) {
        throw new Error('dependency progression resume source is the current output directory');
      }
      const stored = readProgressionState(join(previousRoot, ARTIFACT_FILE.progressionState), {
        progression, featureCatalogIdentity, dependencyPolicyIdentity, owner,
        requireCurrentEngine: true,
      });
      const saved = validateSavedSource(stored, previousRoot);
      state = stored.state;
      resumedStateSha256 = stored.stateSha256;
      priorRun = validatePriorRun(previousRoot, stored);
      snapshotAppSource(saved.path, sourceDirectory);
      restoreAppSource(sourceDirectory, appDir);
      const restored = hashAppSource(appDir);
      if (restored.sha256 !== saved.resume.source.sha256
        || restored.files.length !== saved.resume.source.files) {
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
    return { resumed, action: structuredClone(progressionEngine.nextAction(currentState())),
      status: status(), stateSha256: resumedStateSha256,
      priorRun: priorRun === null ? null : structuredClone(priorRun) };
  };
  const bind = (): ProgressionRecipeAction => {
    const activeState = currentState();
    const binding = recipeBindings.get(activeState.level);
    if (!binding) {
      throw new Error(`dependency progression has no recipe binding for L${activeState.level}`);
    }
    const selected = resolveProgressionRecipeAction(binding, activeState);
    return selected;
  };
  const record = ({ selected, bundle, level, repair, failure = null, repairRegression = null }:
  RecordOptions): ProgressionAction | null => {
    if (!selected || !('grader' in selected)) return null;
    const activeState = currentState();
    const sequence = activeState.attempts.length + 1;
    const evidenceDirectory = join(outputDir, 'progression',
      `attempt-${String(sequence).padStart(3, '0')}`);
    const gradingDirectory = join(appDir, 'stack-bench');
    if (!failure && existsSync(gradingDirectory)) {
      cpSync(gradingDirectory, evidenceDirectory, {
        recursive: true,
        filter: source => !/[\\/]media([\\/]|$)/.test(source),
      });
    }

    let result: (ProgressionGradeResult & {
      repairRegression?: ProgressionRepairRegression;
    }) | CodingFailureResult;
    if (failure) {
      const category = failureCategory(failure.kind) ? failure.kind : 'harness_failure';
      result = {
        attemptId: `${runId}-progression-${sequence}`,
        outcome: 'inconclusive',
        category,
        reason: failure.reason ?? `${category.replaceAll('_', ' ')} during coding`,
      };
    } else if (!object(bundle) || !object(bundle.selection)
      || !existsSync(join(evidenceDirectory, ARTIFACT_FILE.gradeBundle))) {
      result = {
        attemptId: `${runId}-progression-${sequence}`,
        outcome: 'inconclusive',
        category: 'harness_failure',
        reason: 'grader produced no grade bundle',
      };
    } else {
      const source = hashAppSource(appDir);
      const recipe = selected.grader.request.recipe;
      const persistedBundle = readArtifact<GradeBundlePayload>(
        join(evidenceDirectory, ARTIFACT_FILE.gradeBundle), { expectedKind: 'grade_bundle' });
      const gradedResult = gradeBundleToProgressionResult(
        persistedBundle,
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
      result = repairRegression && gradedResult.outcome === 'conclusive'
        ? { ...gradedResult, repairRegression: structuredClone(repairRegression) }
        : gradedResult;
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
          outcome: classifyBundle(artifactPayload(persistedBundle)),
          selectionSha256: selected.grader.selectionSha256,
        });
      }
    }
    state = progressionEngine.recordResult(activeState, result);
    persist({ captureSource: result.outcome === 'conclusive' });
    return progressionEngine.nextAction(currentState());
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
