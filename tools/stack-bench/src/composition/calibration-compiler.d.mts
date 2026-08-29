import type { RecipeCheck, RecipeExecution, RecipeRelease } from './recipe-release.mjs';
import type { Track } from './tracks.mjs';

export interface CalibrationReference {
  backend: string;
  id: string;
  sourceSha256: string;
  status?: string;
  targetPath?: string;
}

export interface CalibrationMutation {
  backend: string;
  path: string;
  sha256: string;
  referenceId: string;
  status?: string;
  executionSha256?: string;
  targets: Array<{ id: string; stableKeys: string[] }>;
}

export interface CalibrationEvidence {
  kind: 'reference' | 'mutation' | 'null';
  stack?: string;
  repetition: number;
  path: string;
  sha256: string;
}

export interface CalibrationControl {
  stableKey: string;
  role: string;
  promotionPolicy: string;
  mutationTargets: string[];
  reason?: string;
}

export interface CalibrationDefinition {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  track: string;
  recipe: {
    path: string;
    id: string;
    version: string;
    meaningSha256: string;
    executionSha256: string;
    contentSha256: string;
  };
  fixture: { id: string; version: string; sourceSha256: string };
  references: { registryPath: string; entries: CalibrationReference[] };
  mutations: CalibrationMutation[];
  nullControl: { pointBearing: string; zeroPoint: string; repetitions: number };
  controls: CalibrationControl[];
  qualification: {
    exactCombinationRequired: boolean;
    referenceRepetitions: number;
    mutationRepetitions: number;
    checks?: string[];
    featureCatalog?: { id: string; version: string; sha256: string };
    runner?: { schemaVersion: number; mode: string; platform: string; architecture: string };
    stacks: Array<{ id: string; status: string }>;
    evidence: CalibrationEvidence[];
    buildImage?: string;
  };
  equivalenceDecisions: Array<{
    fromExecutionSha256: string;
    toExecutionSha256: string;
    rationale: string;
    evidence: Array<{ path: string; sha256: string }>;
  }>;
  qualificationReuse?: {
    sourceRecipe: { id: string; version: string; contentSha256: string; executionSha256: string };
    sourceCalibration: { id: string; version: string; sha256: string };
    rationale: string;
    evidence: Array<{ path: string; sha256: string }>;
    scopes: Array<{
      kind: 'reference' | 'mutation' | 'null';
      stack?: string;
      fromExecutableSha256: string;
      toExecutableSha256: string;
    }>;
  };
  promotion: {
    catalogPath: string;
    catalogSha256: string;
    alias: string;
    coveredAliases?: string[];
    status: string;
  };
}

export interface CalibrationPlan extends CalibrationDefinition {
  contentSha256: string;
  qualificationSha256: string;
  qualificationStaleness: unknown[];
}

export interface CalibrationContext {
  calibration: CalibrationPlan;
  qualificationIdentity: CalibrationIdentity;
  release: RecipeRelease;
  references: CalibrationReference[];
  execution: RecipeExecution[];
  stackBenchRoot: string;
}

export interface CalibrationIdentity {
  id: string;
  version: string;
  sha256: string;
}

export function compileCalibrationDefinition(
  input: unknown,
  options?: { source?: string },
): CalibrationDefinition;

export function compileCalibrationFile(
  path: string,
  options: {
    trackRoot: string;
    stackBenchRoot: string;
    release: RecipeRelease;
  },
): CalibrationPlan;

export function calibrationQualificationIdentity(
  calibration: CalibrationDefinition | CalibrationPlan,
): CalibrationIdentity;

export function calibrationQualificationRelease<T extends Pick<RecipeRelease, 'scoring' | 'checkCatalog'>>(
  calibration: { qualification: { checks?: string[] } },
  release: T,
  execution: RecipeExecution[],
): {
  release: T;
  execution: RecipeExecution[];
};

export function resolveCalibrationForRelease(
  release: RecipeRelease,
  options: { trackRoot: string; stackBenchRoot: string; alias?: string },
): CalibrationPlan | null;

export function calibrationCoversAlias(
  calibration: CalibrationPlan,
  release: RecipeRelease,
  alias: string,
  options: { catalog: unknown; catalogPath: string; trackRoot: string },
): boolean;

export function hasExactSelectedPackRuntime(
  runtime: { packs: Array<{ id: string; exceeded: boolean }> },
  release: { checkCatalog: Array<{ packId?: string }> },
): boolean;

export function currentLevelPoints(
  release: { checkCatalog: Array<Pick<RecipeCheck, 'executionId' | 'points'>> },
  execution: RecipeExecution[],
): number;

export function validateQualificationEvidenceArtifact(
  artifact: unknown,
  entry: CalibrationEvidence,
  context: CalibrationContext,
): void;

export function canReuseQualificationScope(input: unknown): boolean;
