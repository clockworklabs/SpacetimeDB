type MutableRecord = Record<string, unknown>;

const isRecord = (value: unknown): value is MutableRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

export function validateCredentialAliases(
  input: unknown,
  at = 'credentialAliases',
): Readonly<Record<string, string>> {
  if (input === undefined || input === null) return Object.freeze({});
  if (!isRecord(input)) throw new Error(`${at} must be an object`);

  const entries = Object.entries(input);
  const targets = new Set<string>();
  for (const [source, target] of entries) {
    if (!source || typeof target !== 'string' || !target) {
      throw new Error(`${at} must map non-empty strings to non-empty strings`);
    }
    if (source === target) throw new Error(`${at}.${source} must change the credential`);
    if (targets.has(target)) throw new Error(`${at} target ${target} is duplicated`);
    targets.add(target);
  }

  return Object.freeze(Object.fromEntries(
    entries
      .map(([source, target]) => [source, target as string] as const)
      .sort(([left], [right]) => left.localeCompare(right)),
  ));
}

export function applyCredentialAliases(value: unknown, aliases: unknown): string {
  let result = String(value ?? '');
  const entries = Object.entries(validateCredentialAliases(aliases))
    .sort(([left], [right]) => right.length - left.length || left.localeCompare(right));
  for (const [source, target] of entries) result = result.replaceAll(source, target);
  return result;
}

export function materializeScenarioCredentials<T>(input: T, aliases?: unknown): T {
  const resolved = validateCredentialAliases(aliases);
  if (Object.keys(resolved).length === 0) return input;

  const scenario = structuredClone(input);
  const visit = (value: unknown): void => {
    if (Array.isArray(value)) {
      value.forEach(visit);
      return;
    }
    if (!isRecord(value)) return;
    for (const [field, child] of Object.entries(value)) {
      if ((field === 'password' || field === 'text') && typeof child === 'string') {
        value[field] = applyCredentialAliases(child, resolved);
      } else {
        visit(child);
      }
    }
  };
  visit(scenario);
  return scenario;
}
