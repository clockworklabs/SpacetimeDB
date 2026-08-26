function fail(message) {
  throw new Error(`mutation shards: ${message}`);
}

function mutationIds(mutations) {
  if (!Array.isArray(mutations)) fail('mutations must be an array');
  const ids = mutations.map((mutation, index) => {
    if (!mutation || typeof mutation.id !== 'string' || !mutation.id) {
      fail(`mutation ${index} has no id`);
    }
    return mutation.id;
  });
  if (new Set(ids).size !== ids.length) fail('mutation ids must be unique');
  return ids;
}

export function mutationWorkerSlots({ workerCount, runIndex, maxRunIndex }) {
  if (!Number.isInteger(workerCount) || workerCount < 1) {
    fail('worker count must be a positive integer');
  }
  if (!Number.isInteger(runIndex) || runIndex < 0) {
    fail('run index must be a non-negative integer');
  }
  if (!Number.isInteger(maxRunIndex) || maxRunIndex < 0) {
    fail('maximum run index must be a non-negative integer');
  }
  const last = runIndex + workerCount - 1;
  if (last > maxRunIndex) {
    fail(`worker slots ${runIndex}-${last} exceed run-index cap ${maxRunIndex}`);
  }
  return Array.from({ length: workerCount }, (_, index) => runIndex + index);
}

function shardAssignments(mutations, count, defaultScenario) {
  const groups = new Map();
  mutations.forEach((mutation, position) => {
    const scenario = mutation.scenario ?? defaultScenario;
    if (typeof scenario !== 'string' || !scenario.trim()) {
      fail(`mutation ${mutation.id} has no scenario`);
    }
    if (!groups.has(scenario)) groups.set(scenario, { first: position, positions: [] });
    groups.get(scenario).positions.push(position);
  });
  const workers = Array.from({ length: count }, () => ({ size: 0, positions: [] }));
  const groupsBySize = [...groups.values()].sort((a, b) =>
    b.positions.length - a.positions.length || a.first - b.first);
  for (const group of groupsBySize) {
    const worker = workers.reduce((best, candidate) =>
      candidate.size < best.size ? candidate : best, workers[0]);
    worker.positions.push(...group.positions);
    worker.size += group.positions.length;
  }
  for (const worker of workers) worker.positions.sort((a, b) => a - b);
  return workers;
}

export function mutationShard(mutations, { index, count, defaultScenario = null }) {
  const ids = mutationIds(mutations);
  if (!Number.isInteger(count) || count < 1) fail('shard count must be a positive integer');
  if (!Number.isInteger(index) || index < 0 || index >= count) {
    fail(`shard index must be from 0 through ${count - 1}`);
  }
  const positions = shardAssignments(mutations, count, defaultScenario)[index].positions;
  return {
    index,
    count,
    mutationIds: positions.map(position => ids[position]),
    mutations: positions.map(position => mutations[position]),
  };
}

export function mergeMutationShards(mutations, shards, { defaultScenario = null } = {}) {
  const expected = mutationIds(mutations);
  if (!Array.isArray(shards) || shards.length === 0) fail('shards must be a non-empty array');
  const count = shards[0]?.count;
  if (!Number.isInteger(count) || count < 1 || shards.length !== count) {
    fail('shards must contain the declared shard count');
  }
  const byId = new Map();
  const indexes = new Set();
  for (const shard of shards) {
    if (shard?.count !== count || !Number.isInteger(shard.index)
        || shard.index < 0 || shard.index >= count || indexes.has(shard.index)) {
      fail('shard coordinates are invalid or duplicated');
    }
    indexes.add(shard.index);
    const assigned = mutationShard(mutations,
      { index: shard.index, count, defaultScenario }).mutationIds;
    if (JSON.stringify(shard.mutationIds) !== JSON.stringify(assigned)) {
      fail(`shard ${shard.index} does not contain its exact assigned mutation ids`);
    }
    if (!Array.isArray(shard.results) || shard.results.length !== assigned.length) {
      fail(`shard ${shard.index} result count does not match its assignment`);
    }
    for (const result of shard.results) {
      if (!result || typeof result.id !== 'string' || !assigned.includes(result.id)) {
        fail(`shard ${shard.index} contains an unknown result`);
      }
      if (byId.has(result.id)) fail(`mutation ${result.id} appears more than once`);
      byId.set(result.id, result);
    }
  }
  const missing = expected.filter(id => !byId.has(id));
  if (missing.length) fail(`missing mutation results: ${missing.join(', ')}`);
  return expected.map(id => byId.get(id));
}
