// The environment a store gets names each value once, with no frontend tool's prefix.
// Runs recorded before that rule also got Vite-named copies (`vite-names`), and stores
// read them. Saved work keeps the rule its run recorded; no record means `vite-names`.
// The environment carries the rule to child processes, as the source identity rule does.
export type ApplicationEnvironmentRule = 'neutral' | 'vite-names';
export const APPLICATION_ENVIRONMENT_VARIABLE = 'STACK_BENCH_APPLICATION_ENVIRONMENT';

export function applicationEnvironmentRule(env: NodeJS.ProcessEnv = process.env): ApplicationEnvironmentRule {
  const value = env[APPLICATION_ENVIRONMENT_VARIABLE] ?? 'neutral';
  if (value !== 'neutral' && value !== 'vite-names') {
    throw new Error(`${APPLICATION_ENVIRONMENT_VARIABLE} must be neutral or vite-names`);
  }
  return value;
}

export function recordedApplicationEnvironmentRule(setup: unknown): ApplicationEnvironmentRule {
  const recorded = setup !== null && typeof setup === 'object'
    ? (setup as { applicationEnvironment?: unknown }).applicationEnvironment : undefined;
  if (recorded === undefined) return 'vite-names';
  if (recorded !== 'neutral' && recorded !== 'vite-names') throw new Error('recorded application environment rule is invalid');
  return recorded;
}

// The rest of this process continues the saved work under the rule its run recorded.
export function adoptRecordedApplicationEnvironment(setup: unknown): ApplicationEnvironmentRule {
  const rule = recordedApplicationEnvironmentRule(setup);
  process.env[APPLICATION_ENVIRONMENT_VARIABLE] = rule;
  return rule;
}

// Each stack names both sets in its own adapter; this only chooses one.
export function selectApplicationEnvironment(sets: Record<ApplicationEnvironmentRule, Record<string, string>>,
  rule: ApplicationEnvironmentRule = applicationEnvironmentRule()): Record<string, string> {
  return { ...sets[rule] };
}

export function applicationPortEnvironment(port: number,
  rule: ApplicationEnvironmentRule = applicationEnvironmentRule()): Record<string, string> {
  return rule === 'neutral' ? { APP_PORT: String(port) } : { VITE_PORT: String(port) };
}
