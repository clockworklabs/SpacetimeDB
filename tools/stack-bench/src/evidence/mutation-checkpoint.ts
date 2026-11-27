import { isDeepStrictEqual } from 'node:util';

type UnknownRecord = Record<string, unknown>;

export interface MutationCheckpointGroup extends UnknownRecord {
  scenario: string;
  identitySha256: string;
  mutationIds: string[];
}

export interface MutationCheckpointIdentity extends UnknownRecord {
  schemaVersion: 1;
  engineSha256: unknown;
  recipeSha256: unknown;
  fixtureSha256: unknown;
  calibrationSha256: unknown;
  imageId: unknown;
  backend: unknown;
  track: unknown;
  level: unknown;
  trackSha256: unknown;
  shard: unknown;
  groups: MutationCheckpointGroup[];
}

export interface MutationCheckpointResult extends UnknownRecord {
  id: string;
  scenario: string;
}

export interface MutationCheckpointBaseline extends UnknownRecord {
  scenario: string;
  identitySha256: string;
}

export interface MutationCheckpointEvidence {
  checkpoint?: MutationCheckpointIdentity;
  results?: MutationCheckpointResult[];
  baseline?: { scenarios?: MutationCheckpointBaseline[] };
}

const IDENTITY_FIELDS = Object.freeze([
  'engineSha256',
  'recipeSha256',
  'fixtureSha256',
  'calibrationSha256',
  'imageId',
  'backend',
  'track',
  'level',
  'trackSha256',
  'shard',
] as const);

function groupsByScenario(
  groups: readonly MutationCheckpointGroup[],
  at: string,
): Map<string, MutationCheckpointGroup> {
  const result = new Map<string, MutationCheckpointGroup>();
  for (const group of groups) {
    if (result.has(group.scenario)) throw new Error(`${at} duplicates scenario ${group.scenario}`);
    result.set(group.scenario, group);
  }
  return result;
}

export function reusableMutationEvidence(
  prior: MutationCheckpointEvidence | null | undefined,
  identity: MutationCheckpointIdentity,
): { results: MutationCheckpointResult[]; baselines: MutationCheckpointBaseline[] } {
  const checkpoint = prior?.checkpoint;
  if (!checkpoint || checkpoint.schemaVersion !== 1) {
    throw new Error('mutation checkpoint is not resumable');
  }
  for (const field of IDENTITY_FIELDS) {
    if (!isDeepStrictEqual(checkpoint[field], identity[field])) {
      throw new Error(`mutation checkpoint ${field} does not match the current run`);
    }
  }
  const currentGroups = groupsByScenario(identity.groups, 'current mutation groups');
  groupsByScenario(checkpoint.groups ?? [], 'checkpoint mutation groups');
  const reusableScenarios = new Set((checkpoint.groups ?? [])
    .filter(group => currentGroups.get(group.scenario)?.identitySha256 === group.identitySha256)
    .map(group => group.scenario));
  const validMutationIds = new Set(identity.groups
    .filter(group => reusableScenarios.has(group.scenario)).flatMap(group => group.mutationIds));
  const results = (prior.results ?? []).filter(result => validMutationIds.has(result.id)
    && reusableScenarios.has(result.scenario));
  const baselines = (prior.baseline?.scenarios ?? []).filter(baseline =>
    reusableScenarios.has(baseline.scenario)
    && currentGroups.get(baseline.scenario)?.identitySha256 === baseline.identitySha256);
  return { results, baselines };
}
