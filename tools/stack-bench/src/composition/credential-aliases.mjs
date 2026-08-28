const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);

export function validateCredentialAliases(input, at = 'credentialAliases') {
  if (input === undefined || input === null) return Object.freeze({});
  if (!object(input)) throw new Error(`${at} must be an object`);
  const entries = Object.entries(input);
  const targets = new Set();
  for (const [source, target] of entries) {
    if (!source || typeof target !== 'string' || !target) {
      throw new Error(`${at} must map non-empty strings to non-empty strings`);
    }
    if (source === target) throw new Error(`${at}.${source} must change the credential`);
    if (targets.has(target)) throw new Error(`${at} target ${target} is duplicated`);
    targets.add(target);
  }
  return Object.freeze(Object.fromEntries(entries.sort(([left], [right]) =>
    left.localeCompare(right))));
}

export function applyCredentialAliases(value, aliases) {
  let result = String(value ?? '');
  const entries = Object.entries(validateCredentialAliases(aliases))
    .sort(([left], [right]) => right.length - left.length || left.localeCompare(right));
  for (const [source, target] of entries) result = result.replaceAll(source, target);
  return result;
}

export function materializeScenarioCredentials(input, aliases) {
  const resolved = validateCredentialAliases(aliases);
  if (Object.keys(resolved).length === 0) return input;
  const scenario = structuredClone(input);
  const visit = value => {
    if (Array.isArray(value)) {
      value.forEach(visit);
      return;
    }
    if (!object(value)) return;
    for (const [field, child] of Object.entries(value)) {
      if ((field === 'password' || field === 'text') && typeof child === 'string') {
        value[field] = applyCredentialAliases(child, resolved);
      } else visit(child);
    }
  };
  visit(scenario);
  return scenario;
}
