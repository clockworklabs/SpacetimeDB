function sameValue(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

export function reusableMutationEvidence(prior, identity) {
  const checkpoint = prior?.checkpoint;
  if (!checkpoint || checkpoint.schemaVersion !== 1) {
    throw new Error('mutation checkpoint is not resumable');
  }
  for (const field of ['engineSha256', 'recipeSha256', 'fixtureSha256',
    'calibrationSha256', 'imageId', 'backend', 'track', 'level', 'trackSha256', 'shard']) {
    if (!sameValue(checkpoint[field], identity[field])) {
      throw new Error(`mutation checkpoint ${field} does not match the current run`);
    }
  }
  const currentGroups = new Map(identity.groups.map(group => [group.scenario, group]));
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
