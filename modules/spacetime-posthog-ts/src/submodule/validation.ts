import { SenderError } from 'spacetimedb/server';

export const errors = {
  notAuthorized: 'posthog.not_authorized',
  cannotRemoveLastAdmin: 'posthog.cannot_remove_last_admin',
  configMissing: 'posthog.config_missing',
  invalidHost: 'posthog.invalid_host',
  invalidProjectApiKey: 'posthog.invalid_project_api_key',
  invalidDistinctId: 'posthog.invalid_distinct_id',
  invalidEvent: 'posthog.invalid_event',
  invalidPropertiesJson: 'posthog.invalid_properties_json',
  propertiesTooLarge: 'posthog.properties_too_large',
  idempotencyKeyTooLong: 'posthog.idempotency_key_too_long',
  invalidFlagKey: 'posthog.invalid_flag_key',
  invalidPersonPropertiesJson: 'posthog.invalid_person_properties_json',
  personPropertiesTooLarge: 'posthog.person_properties_too_large',
  invalidGroupsJson: 'posthog.invalid_groups_json',
  groupsTooLarge: 'posthog.groups_too_large',
  invalidFlushLimit: 'posthog.invalid_flush_limit',
  invalidClearBatch: 'posthog.invalid_clear_batch',
  invalidRequeueLimit: 'posthog.invalid_requeue_limit',
} as const;

export function throwSenderError(message: string): never {
  throw new SenderError(message);
}

export function normalizeHost(host: string): string {
  const trimmed = host.trim();
  if (!/^https?:\/\/[^/]/i.test(trimmed)) throwSenderError(errors.invalidHost);
  return trimmed.replace(/\/+$/, '');
}

/** Parses a JSON object, returning undefined for invalid JSON or non-object values. */
export function parseJsonObject(json: string): object | undefined {
  try {
    const parsed: unknown = JSON.parse(json);
    return parsed !== null &&
      typeof parsed === 'object' &&
      !Array.isArray(parsed)
      ? parsed
      : undefined;
  } catch {
    return undefined;
  }
}
