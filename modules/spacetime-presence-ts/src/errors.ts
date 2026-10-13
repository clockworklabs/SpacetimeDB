/** SenderError codes thrown by the presence reducers, procedures, and helpers. */
export const errors = {
  notAuthorized: 'presence.not_authorized',
  cannotRemoveLastAdmin: 'presence.cannot_remove_last_admin',
  configMissing: 'presence.config_missing',
  invalidScope: 'presence.invalid_scope',
  invalidSubject: 'presence.invalid_subject',
  invalidStatus: 'presence.invalid_status',
  invalidActivity: 'presence.invalid_activity',
  invalidPayload: 'presence.invalid_payload',
  invalidTtlSeconds: 'presence.invalid_ttl_seconds',
  invalidSweepBatch: 'presence.invalid_sweep_batch',
} as const;
