import { readFileSync } from 'node:fs';
import { isAbsolute } from 'node:path';
import { z } from 'zod';
import { AGENT_ADAPTER_REGISTRY } from './agent-adapters.js';
import { sha256 } from '../evidence/provenance.js';

const label = z.string().regex(/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/);
export const executionCredentialsSchema = z.object({
  default: label.optional(),
  adapters: z.record(z.string().min(1), label).optional(),
  attempts: z.record(z.string().min(1), label).optional(),
}).strict();
export type ExecutionCredentials = z.infer<typeof executionCredentialsSchema>;
export const assignmentSchema = z.object({
  id: label, version: label,
  provider: z.enum(['anthropic', 'openai', 'openrouter']),
  mode: z.enum(['api-key', 'subscription-token']),
}).strict();
export type CredentialAssignment = z.infer<typeof assignmentSchema>;
const profileSchema = assignmentSchema.omit({ id: true }).extend({
  secretFile: z.string().refine(isAbsolute, 'secretFile must be absolute'),
}).strict().refine(profile => profile.provider !== 'openrouter' || profile.mode === 'api-key',
  'OpenRouter requires api-key mode');
const PROFILE_FILE = 'STACK_BENCH_CREDENTIAL_PROFILES_FILE';
const ASSIGNMENT = 'STACK_BENCH_CREDENTIAL_ASSIGNMENT';
const FINGERPRINT = 'STACK_BENCH_CREDENTIAL_SECRET_SHA256';
const authenticationVariables = {
  anthropic: ['ANTHROPIC_API_KEY', 'CLAUDE_CODE_OAUTH_TOKEN'],
  openai: ['OPENAI_API_KEY', 'CODEX_AUTH'],
  openrouter: ['OPENROUTER_API_KEY'],
} as const;

function readProfile(id: string, env: NodeJS.ProcessEnv) {
  const path = env[PROFILE_FILE];
  if (!path || !isAbsolute(path)) throw new Error(`${PROFILE_FILE} must be an absolute path for named credentials`);
  let profiles: unknown;
  try { profiles = JSON.parse(readFileSync(path, 'utf8')); }
  catch { throw new Error('Credential profile registry cannot be read as JSON'); }
  if (!profiles || typeof profiles !== 'object' || Array.isArray(profiles) || !Object.hasOwn(profiles, id)) {
    throw new Error(`Credential profile ${id} is not registered`);
  }
  const parsed = profileSchema.safeParse((profiles as Record<string, unknown>)[id]);
  if (!parsed.success) throw new Error(`Credential profile ${id} has invalid provider, mode, version, or secretFile`);
  const profile = parsed.data;
  let secret: string;
  try { secret = readFileSync(profile.secretFile, 'utf8').trim(); }
  catch { throw new Error(`Credential profile ${id} secret file cannot be read`); }
  if (!secret) throw new Error(`Credential profile ${id} secret file is empty`);
  return { profile, secret, fingerprint: sha256(secret) };
}

export function resolveExecutionCredentials(adapterId: string, attemptId: string,
  assignments: ExecutionCredentials | undefined, source: NodeJS.ProcessEnv = process.env,
): { env: NodeJS.ProcessEnv; assignment: CredentialAssignment | null } {
  const selection = executionCredentialsSchema.parse(assignments ?? {});
  const id = selection.attempts?.[attemptId] ?? selection.adapters?.[adapterId] ?? selection.default;
  const env = { ...source };
  if (!id) return { env, assignment: null };
  const { profile, fingerprint } = readProfile(id, source);
  if (AGENT_ADAPTER_REGISTRY.get(adapterId).provider !== profile.provider) {
    throw new Error(`Credential profile ${id} provider does not match adapter ${adapterId}`);
  }
  const assignment: CredentialAssignment = { id, version: profile.version,
    provider: profile.provider, mode: profile.mode };
  for (const variable of authenticationVariables[profile.provider]) {
    delete env[variable];
    delete env[`${variable}_FILE`];
  }
  // Generic overrides apply to every adapter and must not override an explicit profile.
  delete env.STACK_BENCH_API_KEY_FILE;
  delete env.STACK_BENCH_AGENT_API_KEY;
  delete env.STACK_BENCH_AGENT_API_KEY_FILE;
  const variable = profile.mode === 'api-key' ? authenticationVariables[profile.provider][0]
    : profile.provider === 'anthropic' ? 'CLAUDE_CODE_OAUTH_TOKEN' : 'CODEX_AUTH';
  env[`${variable}_FILE`] = profile.secretFile;
  env[ASSIGNMENT] = JSON.stringify(assignment);
  env[FINGERPRINT] = fingerprint;
  return { env, assignment };
}

/** Fail before a provider invocation if a selected profile changed after admission. */
export function readPinnedExecutionCredential(env: NodeJS.ProcessEnv = process.env):
  { assignment: CredentialAssignment; secretFile: string; secret: string } | null {
  if (!env[ASSIGNMENT] && !env[FINGERPRINT]) return null;
  let assignment: CredentialAssignment;
  try { assignment = assignmentSchema.parse(JSON.parse(env[ASSIGNMENT] ?? '')); }
  catch { throw new Error('Execution credential assignment is invalid'); }
  const { profile, secret, fingerprint } = readProfile(assignment.id, env);
  if (profile.provider !== assignment.provider || profile.mode !== assignment.mode
    || profile.version !== assignment.version || fingerprint !== env[FINGERPRINT]) {
    throw new Error(`Credential profile ${assignment.id} changed after admission; explicitly select the new version before continuing`);
  }
  return { assignment, secretFile: profile.secretFile, secret };
}

export function validateExecutionCredentialTargets(assignments: ExecutionCredentials | undefined,
  adapterIds: readonly string[], attemptIds: readonly string[]): void {
  const selection = executionCredentialsSchema.parse(assignments ?? {});
  for (const id of Object.keys(selection.adapters ?? {})) {
    if (!adapterIds.includes(id)) throw new Error(`Credential assignment names unknown adapter ${id}`);
  }
  for (const id of Object.keys(selection.attempts ?? {})) {
    if (!attemptIds.includes(id)) throw new Error(`Credential assignment names unknown attempt ${id}`);
  }
}
